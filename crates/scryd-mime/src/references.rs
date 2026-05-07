//! Normalize RFC 5322 Message-IDs and the References header to a stable form.
//!
//! Storage's threading walk does case-insensitive matches against
//! `messages.header_message_id`, so all three (header_message_id,
//! in_reply_to, references) need to land lowercased and `<>`-stripped.

/// Strip `<>`, lowercase, and validate as a message-id-shaped token.
/// Returns `None` for malformed inputs (empty, missing the `@` separator).
pub fn normalize_message_id(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let trimmed = trimmed.trim_start_matches('<').trim_end_matches('>').trim();
    if trimmed.is_empty() || !trimmed.contains('@') {
        return None;
    }
    Some(trimmed.to_lowercase())
}

/// Split a References header value on whitespace and normalize each token,
/// silently dropping malformed entries.
pub fn normalize_references(raw: &str) -> Vec<String> {
    raw.split_whitespace()
        .filter_map(normalize_message_id)
        .collect()
}
