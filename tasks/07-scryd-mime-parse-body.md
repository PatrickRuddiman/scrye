Parent slice: [mime-and-markdown](../slices/mime-and-markdown.md)
Depends on: 00

# Task 07 — scryd-mime-parse-body

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Implement the top-level `parse(raw_bytes, ctx)` entry point, body-branch selection (plain ≥100 bytes vs HTML), HTML pre-clean (drop script/style/head/comments/1x1 imgs/inline style), and `htmd` HTML→Markdown conversion.

## Tasks
- [ ] In `crates/scryd-mime/Cargo.toml`, add deps: `mail-parser`, `htmd`, `scraper` (for the HTML pre-clean DOM walk), `serde`, `thiserror`.
- [ ] In `crates/scryd-mime/src/lib.rs`, define the public types from slice §4: `ParseContext { account_id, folder, server_uid, internal_date }`, `ParseOutcome::{Parsed, ParsedDegraded, Unparseable}`, `ParsedMessage { ... }`, `Address { addr, name }`, `AttachmentMeta { filename, mime_type, size_bytes }`, `ParseFault` enum (variants per slice §4: `BodyTruncated { original_bytes }`, `DateFallback { source: &'static str }`, `AttachmentNameUndecodable`, `ParseCapExceeded { cap: &'static str }`, `EncryptedNotIndexed`).
- [ ] In `crates/scryd-mime/src/lib.rs`, define constants matching slice §4: `MIN_PLAIN_BYTES = 100`, `BODY_MAX_BYTES = 1_048_576`, `MAX_PART_DEPTH = 8`, `MAX_PARTS = 1_000`, `MAX_HEADER_BYTES_PER_PART = 65_536`.
- [ ] In `crates/scryd-mime/src/html_clean.rs`, implement `pub fn pre_clean(html: &str) -> String` using `scraper`: parse, walk the DOM, drop `<script>`, `<style>`, `<head>` subtrees and HTML comments; drop `<img>` whose `width <=1 && height <= 1` after attribute parsing; remove `style` attributes from every remaining element; serialize back to a string.
- [ ] In `crates/scryd-mime/src/body.rs`, implement `pub fn select_body(message: &mail_parser::Message) -> (String, Vec<ParseFault>)` per slice §3 Decision 3: if a `text/plain` part has decoded body length ≥ `MIN_PLAIN_BYTES` after `trim()`, return it; else if a `text/html` part exists, run `pre_clean` then `htmd` and return; else `("", vec![])`.
- [ ] In the same file, after body extraction, apply Decision 7: if the resulting string's byte length > `BODY_MAX_BYTES`, truncate at the largest UTF-8 boundary ≤ `BODY_MAX_BYTES - 60` (60 bytes reserved for the sentinel), append `\n\n_[truncated by scryd at 1MB]_`, push a `ParseFault::BodyTruncated { original_bytes }`.
- [ ] In `crates/scryd-mime/src/parse.rs`, implement `pub fn parse(raw_bytes: &[u8], ctx: ParseContext) -> ParseOutcome`. Use `mail_parser::MessageParser::new().parse(raw_bytes)`; if `None`, return `ParseOutcome::Unparseable(vec![ParseFault::ParseCapExceeded { cap: "outer-parse" }])`. Otherwise extract headers, call `select_body`, populate a `ParsedMessage`. Date fallback (Decision 15): use the message Date header → `ctx.internal_date` → `now()` (push `ParseFault::DateFallback { source }`).
- [ ] Write integration tests in `crates/scryd-mime/tests/body.rs` using fixture `.eml` files in `crates/scryd-mime/tests/fixtures/`: a plaintext-only message → body matches plaintext; a HTML-only message → body is Markdown; a multipart/alternative with both → plaintext is selected when ≥100 bytes; a multipart/alternative with a 50-byte plaintext stub → HTML branch is selected; a 2 MB body → truncated, `BodyTruncated` fault present.
- [ ] Write unit tests in `crates/scryd-mime/tests/html_clean.rs`: input HTML with `<script>`, `<style>`, `<!--…-->`, a `<img width="1" height="1">`, and inline `style="color:red"` → all stripped; input with only safe content → unchanged.

## Acceptance criteria
- [ ] `cargo test -p scryd-mime` passes (body + html_clean tests).
- [ ] `cargo check -p scryd-mime` exits 0.
- [ ] `test -d crates/scryd-mime/tests/fixtures` and at least 4 `.eml` fixtures exist.
- [ ] `git grep -nE 'MIN_PLAIN_BYTES\s*=\s*100' crates/scryd-mime/src/lib.rs` matches.
- [ ] `git grep -nE 'BODY_MAX_BYTES\s*=\s*1_048_576' crates/scryd-mime/src/lib.rs` matches.
- [ ] `git grep -nE 'truncated by scryd at 1MB' crates/scryd-mime/src/body.rs` matches.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
