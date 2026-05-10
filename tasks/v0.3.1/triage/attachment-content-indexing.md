# Triage: attachment content indexing

**Status:** Spec §3 Out. Subject lines, sender, body, and attachment file names are indexed; PDF/docx/zip contents are not.

**Why deferred:** Content extraction is its own complexity surface (PDF parsing, OCR, archive recursion). Each format introduces failure modes that don't fit scryd's "small, focused service" shape.

**Triggers:** A consumer whose end-users keep important content in attachments (legal contracts, signed PDFs).

**Sketch:** new `scryd-extract` companion crate / binary that runs in a sandbox, takes raw attachment bytes, returns plain text. scryd-mime calls it for known content types; the extracted text gets indexed alongside the body.
