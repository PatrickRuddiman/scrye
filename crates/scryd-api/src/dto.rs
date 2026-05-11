//! HTTP request/response DTOs. Shapes match the api slice §4 contract.

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Default)]
pub struct SearchQueryDto {
    #[serde(default)]
    pub q: String,
    pub from: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub folder: Option<String>,
    /// Comma-delimited account ids the caller wants the result set
    /// scoped to. Empty / unset = no filter (all accounts).
    pub account_ids: Option<String>,
    pub limit: Option<usize>,
    pub mode: Option<String>,
}

#[derive(Debug, Serialize)]
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

#[derive(Debug, Serialize)]
pub struct SearchResponseDto {
    pub hits: Vec<SearchHitDto>,
    pub mode: String,
    pub elapsed_ms: u64,
}

#[derive(Debug, Serialize)]
pub struct AddressDto {
    pub addr: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AttachmentDto {
    pub filename: String,
    pub mime_type: String,
    pub size_bytes: u64,
}

#[derive(Debug, Serialize)]
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

#[derive(Debug, Serialize)]
pub struct ThreadResponseDto {
    pub thread_id: String,
    pub messages: Vec<MessageDto>,
}

#[derive(Debug, Serialize)]
pub struct AccountDto {
    pub account_id: String,
    pub folders: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct AccountsResponseDto {
    pub accounts: Vec<AccountDto>,
}

#[derive(Debug, Serialize)]
pub struct ErrorDto {
    pub error: ErrorBodyDto,
}

#[derive(Debug, Serialize)]
pub struct ErrorBodyDto {
    pub code: String,
    pub message: String,
}

/// Per spec api §4: limit clamped to 1..=200; floor 200 K_MIN; default 20.
pub const LIMIT_DEFAULT: usize = 20;
pub const LIMIT_MAX: usize = 200;
