//! Attachment enumeration. A part qualifies as an attachment when ANY of:
//!   (a) Content-Disposition is `attachment`
//!   (b) Content-Disposition is `inline` AND a `filename` parameter is set
//!   (c) the top-level MIME type is neither `text` nor `multipart`
//!
//! Only metadata is recorded; payloads stay in the raw `.eml` file.

use mail_parser::{Message, MessagePart, MimeHeaders, PartType};

use crate::{AttachmentMeta, ParseFault};

/// Walk the part tree and return one `AttachmentMeta` per attachment plus
/// any faults encountered while decoding filenames.
pub fn enumerate(message: &Message<'_>) -> (Vec<AttachmentMeta>, Vec<ParseFault>) {
    let mut atts = Vec::new();
    let mut faults = Vec::new();

    // Index 0 is the root container — never an attachment in its own right.
    for (idx, part) in message.parts.iter().enumerate() {
        if idx == 0 {
            continue;
        }
        if !is_attachment_part(part) {
            continue;
        }

        let filename = match attachment_filename(part) {
            Some(f) => f,
            None => {
                faults.push(ParseFault::AttachmentNameUndecodable);
                continue;
            }
        };

        atts.push(AttachmentMeta {
            filename,
            mime_type: part_mime_type(part),
            size_bytes: part_size_bytes(part),
        });
    }

    (atts, faults)
}

fn is_attachment_part(part: &MessagePart<'_>) -> bool {
    if let Some(cd) = part.content_disposition() {
        let t = cd.ctype();
        if t.eq_ignore_ascii_case("attachment") {
            return true;
        }
        if t.eq_ignore_ascii_case("inline") && part.attachment_name().is_some() {
            return true;
        }
    }
    if let Some(ct) = part.content_type() {
        let main = ct.ctype();
        if !main.eq_ignore_ascii_case("text") && !main.eq_ignore_ascii_case("multipart") {
            return true;
        }
    }
    false
}

fn attachment_filename(part: &MessagePart<'_>) -> Option<String> {
    part.attachment_name().map(|s| s.to_string())
}

fn part_mime_type(part: &MessagePart<'_>) -> String {
    let Some(ct) = part.content_type() else {
        return "application/octet-stream".to_string();
    };
    let main = ct.ctype();
    let sub = ct.subtype().unwrap_or("octet-stream");
    format!("{main}/{sub}")
}

fn part_size_bytes(part: &MessagePart<'_>) -> u64 {
    match &part.body {
        PartType::Binary(b) | PartType::InlineBinary(b) => b.len() as u64,
        PartType::Text(s) | PartType::Html(s) => s.len() as u64,
        _ => 0,
    }
}
