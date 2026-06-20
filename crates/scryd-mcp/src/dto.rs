//! MCP tool input/output shapes.
//!
//! Output DTOs are ported from the retired `scryd-api`'s `dto.rs` so MCP
//! clients (and the rewired CLI) keep the wire shapes they already know; each
//! tool serializes one of these to JSON text content. Input arg structs derive
//! `schemars::JsonSchema` so `rmcp` can advertise a tool input schema.
//!
//! The pure conversion helpers (`iso8601_from_unix`, `parse_iso_date`,
//! `message_row_to_dto`, `derive_snippet`, `filter_match`) are also ported here
//! as crate-local functions shared by the read tools.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use scryd_search::Mode;
use scryd_storage::MessageRow;

/// Per spec mcp §4: `limit` clamped to `1..=200`; default 20.
pub const LIMIT_DEFAULT: usize = 20;
pub const LIMIT_MAX: usize = 200;

/// Snippet character budget (ported from the api slice).
pub const SNIPPET_CHARS: usize = 240;

// ---------------------------------------------------------------------------
// Input argument structs (deserialized from MCP tool call params).
// ---------------------------------------------------------------------------

/// Arguments for the `search` tool.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct SearchArgs {
    /// Free-text query. Empty matches everything (subject to filters).
    #[serde(default)]
    pub q: String,
    /// Substring-match the sender address (case-insensitive).
    #[serde(default)]
    pub from: Option<String>,
    /// Inclusive lower date bound, `YYYY-MM-DD`.
    #[serde(default)]
    pub since: Option<String>,
    /// Inclusive upper date bound, `YYYY-MM-DD`.
    #[serde(default)]
    pub until: Option<String>,
    /// Restrict to a single folder.
    #[serde(default)]
    pub folder: Option<String>,
    /// Narrow the result set to these account ids. May only intersect the
    /// caller's owned accounts — never widen them.
    #[serde(default)]
    pub account_ids: Vec<String>,
    /// Max hits to return, clamped to `1..=200`. Default 20.
    #[serde(default)]
    pub limit: Option<usize>,
    /// Search mode: `fulltext` (default) | `semantic` | `hybrid`.
    #[serde(default)]
    pub mode: Option<String>,
}

/// Arguments for the single-id read tools (`get_message`, `get_raw_message`,
/// `get_thread`).
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct IdArg {
    /// The scryd-internal message or thread id.
    pub id: String,
}

// ---------------------------------------------------------------------------
// Search output.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchHitDto {
    pub message_id: String,
    pub account_id: String,
    pub folder: String,
    pub sender_addr: String,
    pub sender_name: Option<String>,
    pub subject: Option<String>,
    pub date: String,
    pub score: f32,
    pub snippet: Option<String>,
    pub thread_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchResponseDto {
    pub hits: Vec<SearchHitDto>,
    pub mode: String,
    pub elapsed_ms: u64,
}

// ---------------------------------------------------------------------------
// Message / thread output.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AddressDto {
    pub addr: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AttachmentDto {
    pub filename: String,
    pub mime_type: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageDto {
    pub message_id: String,
    pub account_id: String,
    pub folder: String,
    pub header_message_id: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub thread_id: String,
    pub from: AddressDto,
    pub to: Vec<AddressDto>,
    pub cc: Vec<AddressDto>,
    pub subject: Option<String>,
    pub date: String,
    pub body_md: String,
    pub attachments: Vec<AttachmentDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThreadResponseDto {
    pub thread_id: String,
    pub messages: Vec<MessageDto>,
}

/// Raw RFC 5322 source for a single message (`get_raw_message`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RawMessageDto {
    pub message_id: String,
    pub raw: String,
}

// ---------------------------------------------------------------------------
// Accounts / status output.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AccountDto {
    pub account_id: String,
    pub folders: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AccountsResponseDto {
    pub accounts: Vec<AccountDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AccountStatusDto {
    pub account_id: String,
    pub folders: Vec<String>,
    pub health: Option<String>,
    pub last_sync_unix: Option<i64>,
    pub last_seen_uid: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LastIndexErrorDto {
    pub message_id: String,
    pub error: String,
    pub attempts: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DrainerStatusDto {
    pub queue_depth: i64,
    pub failed_permanent: i64,
    pub last_index_error: Option<LastIndexErrorDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DaemonStatusDto {
    pub restart_count: i64,
    pub consecutive_crashes: u32,
    pub last_crash_unix: Option<i64>,
    pub in_crash_loop: bool,
    pub in_backoff: bool,
    pub backoff_until_unix: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StatusResponseDto {
    pub ok: bool,
    pub uptime_secs: u64,
    pub accounts: Vec<AccountStatusDto>,
    pub drainer: DrainerStatusDto,
    pub daemon: DaemonStatusDto,
}

// ---------------------------------------------------------------------------
// Pure conversion helpers (ported from scryd-api's response.rs / handlers.rs).
// ---------------------------------------------------------------------------

/// Render a unix timestamp as an RFC 3339 / ISO-8601 string, falling back to
/// the epoch for out-of-range inputs.
pub fn iso8601_from_unix(unix_secs: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(unix_secs, 0)
        .map(|d| d.to_rfc3339())
        .unwrap_or_else(|| {
            chrono::DateTime::<chrono::Utc>::from_timestamp(0, 0)
                .unwrap()
                .to_rfc3339()
        })
}

/// Parse a `YYYY-MM-DD` date to a unix timestamp at 00:00:00 UTC, or `None` if
/// malformed.
pub fn parse_iso_date(s: &str) -> Option<i64> {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|naive| naive.and_utc().timestamp())
}

/// Convert a storage `MessageRow` into the full message DTO.
pub fn message_row_to_dto(row: MessageRow) -> MessageDto {
    let to: Vec<AddressDto> = row.recipients_to.into_iter().map(addr_to_dto).collect();
    let cc: Vec<AddressDto> = row.recipients_cc.into_iter().map(addr_to_dto).collect();
    MessageDto {
        message_id: row.message_id,
        account_id: row.account_id,
        folder: row.folder,
        header_message_id: row.header_message_id,
        in_reply_to: row.in_reply_to,
        references: row.references,
        thread_id: row.thread_id,
        from: AddressDto {
            addr: row.sender_addr,
            name: row.sender_name,
        },
        to,
        cc,
        subject: row.subject,
        date: iso8601_from_unix(row.date_unix),
        body_md: row.body_md,
        // Attachment metadata persistence is out for v1 — the runtime sink
        // doesn't write attachment rows yet. The shape is surfaced so callers
        // don't have to special-case the field.
        attachments: Vec::new(),
    }
}

fn addr_to_dto(a: scryd_storage::Address) -> AddressDto {
    AddressDto {
        addr: a.addr,
        name: a.name,
    }
}

/// Derive the result snippet for a hit, honouring the search mode (semantic
/// hits may carry a pre-rendered chunk; full-text/hybrid render from body).
pub fn derive_snippet(
    body_md: &str,
    semantic_snippet: &Option<String>,
    q: &str,
    mode: Mode,
) -> String {
    match mode {
        Mode::Semantic => semantic_snippet
            .clone()
            .unwrap_or_else(|| scryd_search::snippet::render(body_md, q, SNIPPET_CHARS)),
        Mode::FullText | Mode::Hybrid => {
            scryd_search::snippet::render(body_md, q, SNIPPET_CHARS)
        }
    }
}

/// Post-retrieval filter applied to each candidate hit's row: from-substring,
/// folder, account-scope, and the since/until date window. `account_ids` is the
/// already-scoped (narrowed) allowed set — empty means "no rows pass" only when
/// the caller has already decided to short-circuit; non-empty restricts to
/// those ids.
pub fn filter_match(
    row: &MessageRow,
    args: &SearchArgs,
    account_ids: &[String],
    since_ts: Option<i64>,
    until_ts: Option<i64>,
) -> bool {
    if let Some(from) = args.from.as_deref() {
        if !row.sender_addr.to_lowercase().contains(&from.to_lowercase()) {
            return false;
        }
    }
    if let Some(folder) = args.folder.as_deref() {
        if row.folder != folder {
            return false;
        }
    }
    if !account_ids.is_empty() && !account_ids.iter().any(|id| id == &row.account_id) {
        return false;
    }
    if let Some(s) = since_ts {
        if row.date_unix < s {
            return false;
        }
    }
    if let Some(u) = until_ts {
        if row.date_unix > u {
            return false;
        }
    }
    true
}
