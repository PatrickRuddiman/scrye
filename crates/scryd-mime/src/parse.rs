//! Top-level entry point. Task 07 wires up header extraction + body
//! selection. Task 08 layers attachment enumeration, defensive caps,
//! address normalization, and encrypted/signed handling on top.

use std::time::{SystemTime, UNIX_EPOCH};

use mail_parser::MessageParser;

use crate::body::select_body;
use crate::{Address, ParseContext, ParseFault, ParseOutcome, ParsedMessage};

/// Parse raw RFC 5322 bytes into a structured outcome.
pub fn parse(raw_bytes: &[u8], ctx: ParseContext<'_>) -> ParseOutcome {
    let parser = MessageParser::default();
    let Some(message) = parser.parse(raw_bytes) else {
        return ParseOutcome::Unparseable(vec![ParseFault::ParseCapExceeded {
            cap: "outer-parse",
        }]);
    };

    let mut faults = Vec::new();

    let header_message_id = message.message_id().map(|s| s.to_string());
    let in_reply_to = message
        .in_reply_to()
        .as_text()
        .map(|s| s.to_string());
    let references = message
        .references()
        .as_text_list()
        .map(|list| list.into_iter().map(|s| s.to_string()).collect())
        .unwrap_or_default();

    let subject = message.subject().map(|s| s.to_string());
    let from = first_address(message.from()).unwrap_or(Address {
        addr: "unknown".to_string(),
        name: None,
    });
    let to = flatten_addresses(message.to());
    let cc = flatten_addresses(message.cc());

    let date_unix = resolve_date(&message, &ctx, &mut faults);

    let (body_md, body_faults) = select_body(&message);
    faults.extend(body_faults);

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
        attachments: Vec::new(), // Task 08 fills this in.
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

fn first_address(addr: Option<&mail_parser::Address<'_>>) -> Option<Address> {
    let Some(a) = addr else { return None; };
    iter_addrs(a).into_iter().next()
}

fn flatten_addresses(addr: Option<&mail_parser::Address<'_>>) -> Vec<Address> {
    addr.map(iter_addrs).unwrap_or_default()
}

fn iter_addrs(a: &mail_parser::Address<'_>) -> Vec<Address> {
    let mut out = Vec::new();
    if let Some(list) = a.as_list() {
        for ad in list {
            if let Some(addr_str) = ad.address() {
                out.push(Address {
                    addr: addr_str.to_string(),
                    name: ad.name().map(|n| n.to_string()),
                });
            }
        }
    }
    if let Some(groups) = a.as_group() {
        for g in groups {
            for ad in &g.addresses {
                if let Some(addr_str) = ad.address() {
                    out.push(Address {
                        addr: addr_str.to_string(),
                        name: ad.name().map(|n| n.to_string()),
                    });
                }
            }
        }
    }
    out
}
