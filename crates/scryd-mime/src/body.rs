//! Body-part selection and the truncation sentinel.

use mail_parser::{Message, MessagePart, MimeHeaders, PartType};

use crate::html_clean::pre_clean;
use crate::{ParseFault, BODY_MAX_BYTES, MIN_PLAIN_BYTES};

const TRUNCATION_SENTINEL: &str = "\n\n_[truncated by scryd at 1MB]_";

/// Pick the message body and, if needed, convert HTML→Markdown via `htmd`.
/// Returns the resulting Markdown plus any faults (e.g. truncation).
pub fn select_body(message: &Message<'_>) -> (String, Vec<ParseFault>) {
    let mut faults = Vec::new();

    let plain = first_text_part(message, "plain")
        .map(|s| s.to_string())
        .unwrap_or_default();
    let plain_trimmed_len = plain.trim().len();

    let body = if plain_trimmed_len >= MIN_PLAIN_BYTES {
        plain
    } else if let Some(html) = first_text_part(message, "html") {
        let cleaned = pre_clean(html);
        htmd::convert(&cleaned).unwrap_or_else(|_| cleaned)
    } else if !plain.is_empty() {
        // Plain branch under the threshold but no HTML branch — use what we
        // have rather than dropping the body altogether.
        plain
    } else {
        String::new()
    };

    let body = enforce_size_cap(body, &mut faults);
    (body, faults)
}

fn enforce_size_cap(body: String, faults: &mut Vec<ParseFault>) -> String {
    if body.len() <= BODY_MAX_BYTES {
        return body;
    }
    let original = body.len();
    let head_budget = BODY_MAX_BYTES.saturating_sub(TRUNCATION_SENTINEL.len());
    let cut = utf8_safe_truncate(&body, head_budget);
    let mut out = String::with_capacity(cut + TRUNCATION_SENTINEL.len());
    out.push_str(&body[..cut]);
    out.push_str(TRUNCATION_SENTINEL);
    faults.push(ParseFault::BodyTruncated {
        original_bytes: original,
    });
    out
}

fn utf8_safe_truncate(s: &str, max_bytes: usize) -> usize {
    if max_bytes >= s.len() {
        return s.len();
    }
    let mut idx = max_bytes;
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

fn first_text_part<'a>(message: &'a Message<'a>, subtype: &str) -> Option<&'a str> {
    for part in message.parts.iter() {
        if !is_text_part_with_subtype(part, subtype) {
            continue;
        }
        if let PartType::Text(s) | PartType::Html(s) = &part.body {
            return Some(s.as_ref());
        }
    }
    None
}

fn is_text_part_with_subtype(part: &MessagePart<'_>, subtype: &str) -> bool {
    match &part.body {
        PartType::Text(_) if subtype == "plain" => {
            // mail-parser maps text/plain to PartType::Text by default;
            // double-check the Content-Type if the header is set.
            content_type_subtype_matches(part, "plain")
        }
        PartType::Html(_) if subtype == "html" => content_type_subtype_matches(part, "html"),
        _ => false,
    }
}

fn content_type_subtype_matches(part: &MessagePart<'_>, expected: &str) -> bool {
    let Some(ct) = part.content_type() else {
        // Default per RFC 2046 is text/plain, so an absent Content-Type
        // counts as plain.
        return expected == "plain";
    };
    let Some(sub) = ct.subtype() else { return expected == "plain"; };
    sub.eq_ignore_ascii_case(expected)
}
