//! Top-level entry point. Layers caps, encryption classification, address
//! normalization, references normalization, and attachment enumeration on
//! top of the body / pre-clean pipeline.

use std::time::{SystemTime, UNIX_EPOCH};

use mail_parser::MessageParser;

use crate::addresses;
use crate::attachments;
use crate::body::select_body;
use crate::caps;
use crate::encryption::{self, EncryptionClass, ENCRYPTED_PLACEHOLDER};
use crate::references;
use crate::{Address, ParseContext, ParseFault, ParseOutcome, ParsedMessage};

/// Parse raw RFC 5322 bytes into a structured outcome.
pub fn parse(raw_bytes: &[u8], ctx: ParseContext<'_>) -> ParseOutcome {
    let parser = MessageParser::default();
    let Some(message) = parser.parse(raw_bytes) else {
        return ParseOutcome::Unparseable(vec![ParseFault::ParseCapExceeded {
            cap: "outer-parse",
        }]);
    };

    if let Err(fault) = caps::check_caps(&message) {
        return ParseOutcome::Unparseable(vec![fault]);
    }

    let mut faults = Vec::new();

    let header_message_id = message
        .message_id()
        .and_then(references::normalize_message_id);
    let in_reply_to = message
        .in_reply_to()
        .as_text()
        .and_then(references::normalize_message_id);
    let references = message
        .references()
        .as_text_list()
        .map(|list| {
            list.into_iter()
                .filter_map(references::normalize_message_id)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let subject = message.subject().map(|s| s.to_string());
    let from = message
        .from()
        .and_then(addresses::normalize)
        .unwrap_or_else(|| Address {
            addr: "unknown".to_string(),
            name: None,
        });
    let to = message.to().map(addresses::normalize_list).unwrap_or_default();
    let cc = message.cc().map(addresses::normalize_list).unwrap_or_default();

    let date_unix = resolve_date(&message, &ctx, &mut faults);

    let (body_md, attachments_list) = match encryption::classify(&message) {
        EncryptionClass::PgpEncrypted => {
            faults.push(ParseFault::EncryptedNotIndexed);
            (ENCRYPTED_PLACEHOLDER.to_string(), Vec::new())
        }
        EncryptionClass::Plain | EncryptionClass::SmimeSigned => {
            let (body_md, body_faults) = select_body(&message);
            faults.extend(body_faults);

            let (atts, att_faults) = attachments::enumerate(&message);
            faults.extend(att_faults);
            (body_md, atts)
        }
    };

    let parsed = ParsedMessage {
        header_message_id,
        in_reply_to,
        references,
        subject,
        from,
        to,
        cc,
        date_unix,
        body_md,
        attachments: attachments_list,
    };

    if faults.is_empty() {
        ParseOutcome::Parsed(parsed)
    } else {
        ParseOutcome::ParsedDegraded(parsed, faults)
    }
}

fn resolve_date(
    message: &mail_parser::Message<'_>,
    ctx: &ParseContext<'_>,
    faults: &mut Vec<ParseFault>,
) -> i64 {
    if let Some(d) = message.date() {
        return d.to_timestamp();
    }
    if let Some(internal) = ctx.internal_date {
        faults.push(ParseFault::DateFallback {
            source: "internaldate",
        });
        return internal;
    }
    faults.push(ParseFault::DateFallback { source: "now" });
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
