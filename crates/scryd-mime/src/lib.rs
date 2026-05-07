//! MIME parser, HTML→Markdown conversion, and the closed-set parse-fault
//! enum every consumer routes through.
//!
//! Task 07 lands the body-selection / pre-clean / htmd pipeline.
//! Task 08 layers attachment enumeration, address normalization, references
//! normalization, defensive caps, encrypted/signed handling, and the merged
//! `parse()` entry point on top.

pub mod body;
pub mod html_clean;
pub mod parse;

use serde::{Deserialize, Serialize};

/// Inputs the IMAP layer hands in alongside the raw bytes — used for the
/// date fallback and for tagging fault log lines.
#[derive(Debug, Clone)]
pub struct ParseContext<'a> {
    pub account_id: &'a str,
    pub folder: &'a str,
    pub server_uid: u32,
    /// IMAP `INTERNALDATE` as unix seconds, or `None` if absent.
    pub internal_date: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Address {
    pub addr: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttachmentMeta {
    pub filename: String,
    pub mime_type: String,
    pub size_bytes: u64,
}

/// Closed-set fault enum. Every variant maps to a subtype on the spec's
/// `single-message parse failure` log category.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseFault {
    BodyTruncated { original_bytes: usize },
    DateFallback { source: &'static str },
    AttachmentNameUndecodable,
    ParseCapExceeded { cap: &'static str },
    EncryptedNotIndexed,
}

#[derive(Debug, Clone)]
pub struct ParsedMessage {
    pub header_message_id: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub subject: Option<String>,
    pub from: Address,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub date_unix: i64,
    pub body_md: String,
    pub attachments: Vec<AttachmentMeta>,
}

/// Three-state outcome the storage slice's MessageSink branches on.
#[derive(Debug, Clone)]
pub enum ParseOutcome {
    Parsed(ParsedMessage),
    ParsedDegraded(ParsedMessage, Vec<ParseFault>),
    Unparseable(Vec<ParseFault>),
}

/// Plain-body length floor below which the converter prefers HTML.
pub const MIN_PLAIN_BYTES: usize = 100;

/// Body cap above which the converter truncates with a sentinel.
pub const BODY_MAX_BYTES: usize = 1_048_576;

/// Defensive caps Task 08 enforces in `caps::check_caps`.
pub const MAX_PART_DEPTH: usize = 8;
pub const MAX_PARTS: usize = 1_000;
pub const MAX_HEADER_BYTES_PER_PART: usize = 65_536;

pub use parse::parse;
