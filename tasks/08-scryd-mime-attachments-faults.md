Parent slice: [mime-and-markdown](../slices/mime-and-markdown.md)
Depends on: 07

# Task 08 — scryd-mime-attachments-faults

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Land attachment enumeration, defensive nesting/part/header caps, address normalization, reference-header normalization, encrypted/signed handling, and complete the `ParsedMessage` shape so storage can persist a full row.

## Tasks
- [x] In `crates/scryd-mime/src/attachments.rs`, implement `pub fn enumerate(message: &mail_parser::Message) -> Vec<AttachmentMeta>` per slice §3 Decision 10: a part is an attachment when ANY of (a) Content-Disposition is `attachment`, (b) Content-Disposition is `inline` with a `filename` parameter, (c) top-level type is not `text` and not `multipart`.
- [x] In the same file, decode RFC 2047 filenames; on undecodable, push a `ParseFault::AttachmentNameUndecodable` (returned through a tuple shape `(Vec<AttachmentMeta>, Vec<ParseFault>)`).
- [x] In `crates/scryd-mime/src/inline_images.rs`, implement `pub fn substitute_inline_image_placeholders(html: &str, parts: &mail_parser::Message) -> String` that, for each `<img src="cid:..." ...>` reference matching a part's Content-ID, replaces the `<img>` element with `[image: <filename-or-cid>]`. Run this AFTER `pre_clean` and BEFORE `htmd`.
- [x] In `crates/scryd-mime/src/addresses.rs`, implement `pub fn normalize(addr: &mail_parser::Address) -> Address` that lowercases the addr-spec and decodes the display name. `normalize_list` for To/Cc lists.
- [x] In `crates/scryd-mime/src/references.rs`, implement `pub fn normalize_message_id(raw: &str) -> Option<String>` (strip `<>`, lowercase, return `None` on malformed) and `pub fn normalize_references(raw: &str) -> Vec<String>` (split on whitespace, normalize each, drop `None`s).
- [x] In `crates/scryd-mime/src/caps.rs`, implement `pub fn check_caps(message: &mail_parser::Message) -> Result<(), ParseFault>` walking the part tree: count parts and depth; on any cap exceeded, return `ParseFault::ParseCapExceeded { cap: <name> }` (cap names: `"part-depth"`, `"part-count"`, `"header-bytes-per-part"`).
- [x] In `crates/scryd-mime/src/encryption.rs`, implement `pub fn classify(message: &mail_parser::Message) -> EncryptionClass` returning `Plain | PgpEncrypted | SmimeSigned`. The `parse` entry point uses this: PGP → `body_md = "[encrypted message — not indexed by scryd]"`, attachments empty, push `EncryptedNotIndexed`. S/MIME signed → recurse into the inner part as the body source.
- [x] Update `crates/scryd-mime/src/parse.rs` from Task 07 to: (a) call `caps::check_caps` early and return `Unparseable` on failure; (b) call `encryption::classify` and branch per Decision 13; (c) call `addresses::normalize` for From/To/Cc; (d) call `references::normalize_*` for In-Reply-To / References; (e) call `attachments::enumerate` and merge any faults; (f) compose the final `ParseOutcome`.
- [x] Write integration tests in `crates/scryd-mime/tests/attachments.rs` with fixture `.eml`s: a message with one PDF attachment → one `AttachmentMeta { filename: "invoice.pdf", mime_type: "application/pdf", size_bytes: <n> }`; an inline image with cid: → image is in attachments AND replaced in body with `[image: ...]`; an attachment with a `=?UTF-8?B?...?=` filename → decoded.
- [x] Write integration tests in `crates/scryd-mime/tests/encryption.rs` with fixture `.eml`s: a `multipart/encrypted` PGP message → body is the placeholder, `EncryptedNotIndexed` fault is returned, headers extracted; a `multipart/signed` S/MIME message → inner body is parsed normally.
- [x] Write integration tests in `crates/scryd-mime/tests/caps.rs`: synthesize a `multipart/mixed` 9-deep nesting → `Unparseable` with `ParseCapExceeded { cap: "part-depth" }`; synthesize 1001 parts → `Unparseable` with `cap: "part-count"`.
- [x] Write unit tests in `crates/scryd-mime/tests/references.rs`: `<ABC@x>` → `"abc@x"`; `<a@x> <B@y>` (References) → `["a@x", "b@y"]`; `not-a-msg-id` → `None`.

## Acceptance criteria
- [x] `cargo test -p scryd-mime` passes (all integration tests added in Task 07 + this task).
- [x] `cargo check -p scryd-mime` exits 0.
- [x] `git grep -nE 'encrypted message — not indexed by scryd' crates/scryd-mime/src/encryption.rs` matches.
- [x] `git grep -nE 'MAX_PART_DEPTH\s*=\s*8|MAX_PARTS\s*=\s*1_000|MAX_HEADER_BYTES_PER_PART\s*=\s*65_536' crates/scryd-mime/src/lib.rs | wc -l` outputs `3`.
- [x] `git grep -nE 'fn enumerate' crates/scryd-mime/src/attachments.rs` matches.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
