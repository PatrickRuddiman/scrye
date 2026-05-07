//! Defensive caps. A malformed (or hostile) message that nests too deeply,
//! ships too many parts, or carries too many header bytes per part is
//! refused before it reaches the indexing pipeline.

use mail_parser::{Message, PartType};

use crate::{ParseFault, MAX_HEADER_BYTES_PER_PART, MAX_PARTS, MAX_PART_DEPTH};

/// Walk the part tree and reject anything beyond the spec's caps.
pub fn check_caps(message: &Message<'_>) -> Result<(), ParseFault> {
    if message.parts.len() > MAX_PARTS {
        return Err(ParseFault::ParseCapExceeded { cap: "part-count" });
    }

    for part in &message.parts {
        let bytes: usize = part
            .headers
            .iter()
            .map(|h| h.name.as_str().len() + h.offset_end.saturating_sub(h.offset_field))
            .sum();
        if bytes > MAX_HEADER_BYTES_PER_PART {
            return Err(ParseFault::ParseCapExceeded {
                cap: "header-bytes-per-part",
            });
        }
    }

    if part_depth(message) > MAX_PART_DEPTH {
        return Err(ParseFault::ParseCapExceeded { cap: "part-depth" });
    }

    Ok(())
}

fn part_depth(message: &Message<'_>) -> usize {
    if message.parts.is_empty() {
        return 0;
    }
    walk(&message.parts, 0, 1)
}

fn walk(parts: &[mail_parser::MessagePart<'_>], idx: usize, depth: usize) -> usize {
    if idx >= parts.len() {
        return depth;
    }
    let mut max_d = depth;
    if let PartType::Multipart(children) = &parts[idx].body {
        for &c in children {
            max_d = max_d.max(walk(parts, c, depth + 1));
        }
    }
    max_d
}
