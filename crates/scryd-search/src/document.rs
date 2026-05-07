//! Build the document text the indexer indexes for a message.
//!
//! Concatenation per search-engine slice §3 Decision 6:
//! `<subject>\n\n<from-display>\n\n<body-markdown>`. Empty subject is
//! omitted; from-display is `name <addr>` if a name exists, otherwise
//! the bare address.

/// Compose a single string for the indexer to ingest.
pub fn build(
    subject: Option<&str>,
    sender_addr: &str,
    sender_name: Option<&str>,
    body_md: &str,
) -> String {
    let mut out = String::new();
    if let Some(s) = subject {
        let s = s.trim();
        if !s.is_empty() {
            out.push_str(s);
            out.push_str("\n\n");
        }
    }
    out.push_str(&from_display(sender_addr, sender_name));
    out.push_str("\n\n");
    out.push_str(body_md);
    out
}

fn from_display(addr: &str, name: Option<&str>) -> String {
    match name {
        Some(n) if !n.trim().is_empty() => format!("{} <{}>", n.trim(), addr),
        _ => addr.to_string(),
    }
}
