//! Replace `cid:`-referenced images with `[image: …]` placeholders in the
//! Markdown body. Substitution runs *after* `htmd::convert` against
//! `![alt](cid:…)` markdown — running before htmd lets the converter
//! escape the brackets (`\[image: cat\]`), which corrupts the placeholder.

use std::collections::HashMap;
use std::sync::LazyLock;

use mail_parser::{Message, MimeHeaders};
use regex::Regex;

static MD_CID_IMG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"!\[[^\]]*\]\(cid:([^)\s]+)\)"#).unwrap());

/// Substitute every `![alt](cid:CID)` Markdown image with
/// `[image: <filename-or-cid>]`. Filename comes from the matching part's
/// Content-ID lookup; falls back to the bare CID.
pub fn substitute_inline_image_placeholders(md: &str, message: &Message<'_>) -> String {
    let cid_to_label = build_cid_label_map(message);
    MD_CID_IMG
        .replace_all(md, |caps: &regex::Captures<'_>| {
            let cid = strip_brackets(&caps[1]);
            let label = cid_to_label
                .get(cid.as_str())
                .map(String::as_str)
                .unwrap_or(cid.as_str());
            format!("[image: {label}]")
        })
        .into_owned()
}

fn build_cid_label_map(message: &Message<'_>) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for part in message.parts.iter() {
        let Some(cid) = part.content_id() else { continue };
        let cid = strip_brackets(cid);
        let label = part
            .attachment_name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| cid.clone());
        map.insert(cid, label);
    }
    map
}

fn strip_brackets(s: &str) -> String {
    s.trim().trim_start_matches('<').trim_end_matches('>').trim().to_string()
}
