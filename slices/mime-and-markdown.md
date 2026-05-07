Parent spec: [scryd-spec.md](../scryd-spec.md)

# scryd — mime-and-markdown

## §1 Summary

Owns turning the raw `.eml` bytes that imap-sync hands in into the structured shape storage persists: decoded headers (subject, from, to, cc, date, message-id, in-reply-to, references), a Markdown-rendered body, and an attachment metadata list (no payloads). Every parser-side promise the spec makes — body returned as Markdown, attachment list with `(filename, MIME type, size)`, the `single-message parse failure` failure mode — lands here.

## §2 Codebase reconnaissance

> Greenfield: no existing system to reconcile. Decisions below are unconstrained.

External Rust crates this slice leans on (versions pinned at coding time, in build-and-packaging):

- `mail-parser` — robust MIME / multipart / charset / encoded-word handling. Spec already named it.
- `html2md` (or equivalent: `nom-html2md`, `htmd`) — converts HTML to Markdown. Decision 2 picks the specific crate.
- `idna` for IDN-encoded address domains, if any addresses use them; `mail-parser` already normalizes most of this internally.

## §3 Decisions

1. **MIME parser.** `mail-parser`. Rationale: pure Rust, handles RFC 5322 + 2045/2046/2047 + nested multipart + every charset alias the wild web uses; spec already named it; no widely used alternative.
2. **HTML→Markdown converter.** `html2md` (or the closest pure-Rust equivalent that survives the build-and-packaging pin). Rationale: produces Markdown directly (not plain text), preserving links and structure that the search index and the message-fetch response benefit from. We do not need fidelity for re-rendering; we need structure-preserving readable text.
3. **Body-part selection.** If the message has a `text/plain` part whose decoded body is at least `MIN_PLAIN_BYTES` (default 100 bytes after trim), use it as the body directly. Otherwise, if the message has a `text/html` part, run it through HTML pre-clean (Decision 6) then `html2md`. If neither, body is the empty string. Rationale: plaintext branches are usually authored versions and contain less marketing noise; the 100-byte threshold guards against the common "View this email in your browser" stub that some senders put in the plaintext branch.
4. **Charset normalization.** All extracted text is decoded to UTF-8 by `mail-parser`'s charset handling; we never expose non-UTF-8 bytes downstream. Rationale: storage's `body_md` is `TEXT`; SQLite assumes UTF-8.
5. **Encoded-word headers.** Subject, From, To, Cc, In-Reply-To, References are RFC 2047 decoded by `mail-parser` before they reach storage. Rationale: spec promises these as plain user-readable fields.
6. **HTML pre-clean before Markdown conversion.** Before passing HTML to the converter, strip `<script>`, `<style>`, HTML comments, tracking pixels (`<img>` with `width <= 1` and `height <= 1`), inline `style="…"` attributes, and `<head>` entirely. Rationale: marketing email HTML is heavy with these and they produce no useful tokens for search; removing them reduces body_md size and indexer load without losing semantic content.
7. **Body length cap.** After Markdown conversion, if the body exceeds `BODY_MAX_BYTES` (default 1 MB), truncate at a UTF-8 boundary, append a sentinel line `\n\n_[truncated by scryd at 1MB]_`, log a `single-message parse failure` entry of subtype `body-truncated`. Rationale: storage slice's soft inline-body limit is 1 MB; pathological bodies don't bloat the row and don't slow indexing. The truncation log keeps the operator informed.
8. **Address normalization.** For each address (From / per-recipient in To, Cc): keep `display-name` in the `name` field decoded UTF-8 (or NULL); lowercased `addr-spec` (the `local@domain`) in the `addr` field. From's primary address feeds `messages.sender_addr` and `messages.sender_name`; To/Cc are JSON-serialized into `recipients_to_json` / `recipients_cc_json` per the storage slice's shape. Rationale: spec promises sender filtering; lowercasing makes filter matches consistent across providers' inconsistent casing.
9. **Reference-header normalization.** `In-Reply-To`: extract the first `<…>` token, strip `<>`, lowercase, store. `References`: split on whitespace, for each `<…>` token strip `<>` and lowercase, output as a JSON array. Rationale: storage's threading walk in §3 Decision 8 of the storage slice does case-insensitive matching against `messages.header_message_id`; normalizing here keeps the join cheap.
10. **Attachment detection.** A part is an "attachment" for the spec's purposes when ANY of: (a) it has a `Content-Disposition: attachment`, (b) it has a `Content-Disposition: inline` with a `filename` parameter, (c) its top-level MIME type is not `text` and not `multipart`. Rationale: covers the three real-world cases; `text/plain` and `text/html` parts that are the body itself are excluded by the type check; `multipart/related` inline images with a filename are surfaced as attachments so callers can list them, but their cid: references in the HTML body are not embedded in the Markdown body.
11. **Attachment metadata only — no payloads.** For each attachment: record `filename` (RFC 2047 decoded), `mime_type`, `size_bytes` (size of the encoded payload as it appeared in the raw eml). Rationale: spec is explicit — no payloads in v1; raw bytes remain available via the `.eml` file in the raw store.
12. **Inline-image placeholder.** When an inline image with a `Content-ID` is referenced from the HTML body via `cid:`, the converter replaces the `<img>` with `[image: <filename-or-cid>]` in Markdown. Rationale: keeps the body readable; the attachment metadata list separately exposes the image to the caller.
13. **Encrypted/signed handling.**
    - PGP-encrypted (`multipart/encrypted` or `application/pgp-encrypted`): `body_md = "[encrypted message — not indexed by scryd]"`, no attachments expanded from the encrypted blob, structural headers stored normally. Rationale: no decryption in v1; honesty about what's indexed; nothing to search means an empty body_md would be misleading.
    - S/MIME-signed (`multipart/signed`): parse the inner part normally as if the signature wrapper were absent; signed-payload-only attachments enumerate as attachments. Rationale: signature wrappers carry no body content; the inner body is the human-readable message.
14. **Defensive parse caps.** Maximum nesting depth `8`, maximum total parts `1000`, maximum single attachment header size `64 KB`. Beyond any cap, abort the parse with a `single-message parse failure` log of subtype `parse-cap-exceeded`. Headers extracted before the cap was hit are still stored. Rationale: pathological multipart inputs (intentional or otherwise) shouldn't burn CPU/memory or wedge the indexer queue.
15. **Date header parsing.** `mail-parser`'s parsed `Date` is converted to a unix epoch second. If the header is absent or unparseable, fall back to the IMAP `INTERNALDATE` from the FetchedMessage. If both are missing or unparseable, fall back to the daemon's current wall-clock at parse time and log a `single-message parse failure` of subtype `date-fallback`. Rationale: storage requires a non-NULL `date_unix`; we should never reject a message for a bad date.
16. **Parse error policy.** Any parse failure that prevents producing a `ParsedMessage` value emits a `single-message parse failure` log entry tagged with the originating account, folder, and server-side UID, and storage receives a placeholder: empty `body_md`, no attachments, headers extracted to whatever extent succeeded (`subject`, `sender_addr`, `sender_name` may be `Unknown` strings). The raw `.eml` is still written by storage; the message is still queryable by date and folder. Rationale: the spec's failure mode for parse failures is to skip the message body but keep sync moving; this is the operationalization.

## §4 Contracts & shapes

Internal Rust crate (provisional name): `scryd-mime`.

Public surface:

- `parse(raw_bytes: &[u8], context: ParseContext) -> ParseOutcome`
  - `ParseContext { account_id: &str, folder: &str, server_uid: u32, internal_date: Option<UnixSeconds> }`
  - `ParseOutcome` is one of:
    - `Parsed(ParsedMessage)` — at least the structural headers and the body extraction succeeded.
    - `ParsedDegraded(ParsedMessage, Vec<ParseFault>)` — produced a usable result but specific subsystems hit a fault (truncation, date-fallback, attachment-decoded-as-bytes, etc.).
    - `Unparseable(Vec<ParseFault>)` — the message could not be parsed at all; storage will still write the raw `.eml` and a header-only placeholder.
- `ParsedMessage`:
  - `header_message_id: Option<String>` — lowercased, `<>`-stripped, or `None` if absent/malformed.
  - `in_reply_to: Option<String>`
  - `references: Vec<String>`
  - `subject: Option<String>`
  - `from: Address` where `Address { addr: String /* lowercased */, name: Option<String> }`
  - `to: Vec<Address>`
  - `cc: Vec<Address>`
  - `date_unix: i64`
  - `body_md: String`
  - `attachments: Vec<AttachmentMeta>`
- `AttachmentMeta { filename: String, mime_type: String, size_bytes: u64 }`
- `ParseFault` — enum: `BodyTruncated { original_bytes }`, `DateFallback { source: "internaldate"|"now" }`, `AttachmentNameUndecodable`, `ParseCapExceeded { cap: &'static str }`, `EncryptedNotIndexed`, …; each variant maps to a log subtype on the spec's `single-message parse failure` category.

Constants:

- `MIN_PLAIN_BYTES = 100`
- `BODY_MAX_BYTES = 1_048_576` (1 MB)
- `MAX_PART_DEPTH = 8`
- `MAX_PARTS = 1_000`
- `MAX_HEADER_BYTES_PER_PART = 65_536`

HTML pre-clean rules (Decision 6) — exhaustive list:

- Drop entire `<script>` subtree.
- Drop entire `<style>` subtree.
- Drop entire `<head>` subtree.
- Drop HTML comments.
- Drop `<img>` whose `width <= 1` AND `height <= 1` after attribute parsing.
- Drop `style="…"` attributes from every remaining element.

## §5 Sequence

1. **Storage receives a `FetchedMessage` from imap-sync.** Storage calls `scryd-mime::parse(raw_bytes, ctx)`.
2. **Top-level parse.** `mail-parser` parses the byte stream into a header set + a part tree. If this fails outright (truncated bytes, header overflow, depth/part-cap exceeded), return `Unparseable(faults)` with the appropriate `ParseFault`.
3. **Header extraction.** Decode encoded-word headers (Decision 5), normalize Reference / In-Reply-To (Decision 9), normalize From/To/Cc (Decision 8). Resolve `date_unix` (Decision 15).
4. **Body branch selection.** Walk the part tree:
   - Identify the primary plaintext part (top-level `text/plain` or first `text/plain` inside a `multipart/alternative`). If its decoded length ≥ `MIN_PLAIN_BYTES`, take it as `body_md` and skip HTML conversion.
   - Otherwise, identify the primary HTML part. Run it through HTML pre-clean (Decision 6), then `html2md`, store the result as `body_md`.
   - Otherwise, `body_md = ""`.
5. **Body cap.** If `body_md.len() > BODY_MAX_BYTES`, truncate at a UTF-8 boundary, append the sentinel, push a `BodyTruncated` fault.
6. **Attachment enumeration.** Walk the part tree again; for each part matching Decision 10 criteria, record an `AttachmentMeta` and (if it had a Content-ID inline-referenced from the HTML body) ensure the converter substituted the placeholder per Decision 12.
7. **Encrypted/signed branches.** If the top-level structure matches Decision 13's encrypted shape, set `body_md` to the encrypted-placeholder string and push an `EncryptedNotIndexed` fault. If signed, transparently parse the inner part as the body source.
8. **Return.** `Parsed(...)` if no faults, `ParsedDegraded(..., faults)` if any faults accumulated, `Unparseable(faults)` if no `ParsedMessage` could be produced.
9. **Storage handles the outcome.** On `Parsed` / `ParsedDegraded`: storage writes the message row, attachments rows, raw `.eml` file, and inserts the index_queue row. On `Unparseable`: storage writes a placeholder messages row (empty `body_md`, zero attachments, whatever headers were extractable from a salvage pass), writes the raw `.eml`, does NOT insert into index_queue (no body to index).
10. **Each fault becomes a log entry.** observability writes one `single-message parse failure` line per fault, tagged with subtype, account_id, folder, server_uid, and the scryd MessageId once known.

## §6 Out of scope

- Storing or retrieving the raw `.eml` bytes (storage slice).
- The `messages` and `attachments` schema (storage slice).
- IMAP fetching itself (imap-sync slice).
- Witchcraft document formatting and indexing (search-engine slice).
- Snippet derivation (search-engine slice).
- Decryption of PGP / S/MIME — v1 does not decrypt.
- Attachment text extraction (PDF, docx, etc.) — explicitly out per spec §3 Out.
- Boilerplate / marketing-stripping heuristics — explicitly out per spec §5 risks (v2).
- Per-sender rendering rules.
- Localization of any sentinel strings the parser injects.

## §7 Open questions

- The `html2md` crate ecosystem has multiple competing names (`html2md`, `htmd`, `nom-html2md`, hand-rolled via `scraper`). The exact crate is pinned in build-and-packaging; this slice commits to the contract, not the crate identity. Confirm during the first task here that the chosen crate honors the HTML pre-clean produced output (some converters re-introduce noise from default styles).
- Whether the body-branch selection should fall back to text/html when plaintext is non-empty but is detectably a stub like "View this email in your browser…" (i.e., regex match on a few known patterns). v1 keeps the simple 100-byte threshold; revisit if marketing-noise complaints appear.
- Whether a part with `Content-Disposition` absent and a textual MIME type other than `text/plain` and `text/html` (e.g., `text/calendar`) should be treated as an attachment or inlined. v1: such parts are treated as attachments (Decision 10 catch-all on top-level type ≠ `text` AND ≠ `multipart` would NOT fire here because `text/calendar` is `text/`). Refine: extend Decision 10 to "the part is `text/*` but is not the primary plaintext or HTML body" → attachment. Confirm before coding.
- Whether the parser should hash the raw bytes and emit the hash in `ParsedMessage` for use as a duplicate-detection key across folders within an account. v1: no — spec's open question on cross-folder dedup defers to v2; storage's `(account_id, folder, server_uid, uidvalidity)` uniqueness is sufficient for v1 correctness.

> If the parent spec is ambiguous on anything this slice depends on, stop and update the spec. Do not invent behavior here.
