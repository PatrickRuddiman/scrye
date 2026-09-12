---
sources:
  - crates/scryd-mime/src/lib.rs
  - crates/scryd-mime/src/parse.rs
  - crates/scryd-mime/src/body.rs
  - crates/scryd-mime/src/html_clean.rs
  - crates/scryd-mime/src/inline_images.rs
  - crates/scryd-mime/src/attachments.rs
  - crates/scryd-mime/src/encryption.rs
  - crates/scryd-mime/src/caps.rs
  - crates/scryd-mime/src/references.rs
  - crates/scryd-mime/src/addresses.rs
---

# MIME and Markdown

Before a message is stored or indexed, scryd parses its raw MIME into typed
headers and a Markdown body. This page covers that pipeline, the body-selection
rules, the defensive caps, and how encrypted mail is handled.

## Parse outcome

`parse()` returns a three-state outcome that the storage layer branches on:

| Outcome | Meaning |
| --- | --- |
| `Parsed` | Fully parsed with no faults. |
| `ParsedDegraded` | Parsed, but with one or more recorded faults. |
| `Unparseable` | Could not be parsed; only faults are returned. |

A parsed message carries the message-id, `In-Reply-To`, `References`, subject,
`From`/`To`/`Cc` addresses, a unix date, the Markdown body, and attachment
metadata.

## Body selection

scryd stores a Markdown body (`body_md`) for every message:

- If the message has a plain-text body of at least `MIN_PLAIN_BYTES` (100 bytes),
  that text is used.
- Otherwise the HTML body is cleaned and converted to Markdown.
- A body larger than `BODY_MAX_BYTES` (1,048,576 bytes) is truncated with a
  sentinel and a `BodyTruncated` fault is recorded.

## Defensive caps

A malformed or hostile message is refused before it reaches the indexer
(`caps.rs`). Each breach records a `ParseCapExceeded` fault with a `cap` string:

| Cap | Limit | `cap` string |
| --- | --- | --- |
| Parts per message | `MAX_PARTS` = 1,000 | `part-count` |
| Header bytes per part | `MAX_HEADER_BYTES_PER_PART` = 65,536 | `header-bytes-per-part` |
| Part nesting depth | `MAX_PART_DEPTH` = 8 | `part-depth` |

## Parse faults

`ParseFault` is a closed set. Every variant maps to a subtype of the
`single-message parse failure` log category (see [Observability](observability.md)):

| Fault | Meaning |
| --- | --- |
| `BodyTruncated { original_bytes }` | The body exceeded `BODY_MAX_BYTES` and was cut. |
| `DateFallback { source }` | The `Date` header was missing or unparseable; a fallback was used. |
| `AttachmentNameUndecodable` | An attachment filename could not be decoded. |
| `ParseCapExceeded { cap }` | A defensive cap was exceeded (above). |
| `EncryptedNotIndexed` | An encrypted body was not indexed (below). |

## Encryption

scryd classifies the top-level container by its `Content-Type` (`encryption.rs`):

| Class | Trigger | Handling |
| --- | --- | --- |
| `Plain` | Anything not below | Parsed normally. |
| `PgpEncrypted` | `multipart/encrypted` | Body is replaced with the placeholder `[encrypted message — not indexed by scryd]`, no attachments are recorded, and an `EncryptedNotIndexed` fault is set. |
| `SmimeSigned` | `multipart/signed` | Parsed normally; the parser flattens the signature wrapper. |

An encrypted message is still stored and searchable by its headers (sender,
subject, date); only its body content is not indexed.

## See also

- [Architecture](architecture.md)
- [Storage](storage.md)
- [Indexing and search](indexing-and-search.md)
- [Observability](observability.md)
- [get_message tool](../mcp/tools/get_message.md)
