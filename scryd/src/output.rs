//! Plain-text rendering of `/search` responses for the operator's
//! terminal. ANSI bold for `**...**` markup when stdout is a TTY, literal
//! `**...**` when piped.

use serde_json::Value;

const ANSI_BOLD_OPEN: &str = "\u{001b}[1m";
const ANSI_BOLD_CLOSE: &str = "\u{001b}[22m";

/// Render the `/search` JSON response per cli slice §4.
/// Hit lines: `[<account>] <YYYY-MM-DD> <Name> <<addr>> · <subject>`
/// then the snippet indented by four spaces.
/// Trailing summary: `<n> hits in <ms>ms`.
pub fn render_search(resp: &Value, tty: bool) -> String {
    let hits = resp["hits"].as_array().cloned().unwrap_or_default();
    let elapsed_ms = resp["elapsed_ms"].as_u64().unwrap_or(0);

    let mut out = String::new();
    for hit in &hits {
        let account = hit["account_id"].as_str().unwrap_or("?");
        let date = hit["date"].as_str().unwrap_or("");
        let date_short = date.get(..10).unwrap_or(date);
        let addr = hit["sender_addr"].as_str().unwrap_or("");
        let name = hit["sender_name"].as_str();
        let subject = hit["subject"].as_str().unwrap_or("(no subject)");
        let snippet = hit["snippet"].as_str().unwrap_or("");
        let from_display = match name {
            Some(n) if !n.trim().is_empty() => format!("{n} <{addr}>"),
            _ => addr.to_string(),
        };
        out.push_str(&format!(
            "[{account}] {date_short} {from_display} · {subject}\n"
        ));
        let rendered_snippet = render_snippet_markup(snippet, tty);
        out.push_str(&format!("    {rendered_snippet}\n"));
    }

    if !hits.is_empty() {
        out.push('\n');
    }
    out.push_str(&format!("{} hits in {}ms\n", hits.len(), elapsed_ms));
    out
}

/// Convert `**foo**` snippet markup to ANSI bold when on a TTY, leave
/// literal otherwise.
fn render_snippet_markup(snippet: &str, tty: bool) -> String {
    if !tty {
        return snippet.to_string();
    }
    // Replace pairs of `**` left-to-right. We don't try to handle
    // mismatched markers — the snippet generator emits balanced pairs.
    let mut out = String::with_capacity(snippet.len() + 16);
    let mut in_bold = false;
    let mut i = 0;
    while i < snippet.len() {
        if snippet.as_bytes().get(i) == Some(&b'*') && snippet.as_bytes().get(i + 1) == Some(&b'*')
        {
            if in_bold {
                out.push_str(ANSI_BOLD_CLOSE);
            } else {
                out.push_str(ANSI_BOLD_OPEN);
            }
            in_bold = !in_bold;
            i += 2;
            continue;
        }
        let next = snippet
            .char_indices()
            .find(|(idx, _)| *idx > i)
            .map(|(idx, _)| idx)
            .unwrap_or(snippet.len());
        out.push_str(&snippet[i..next]);
        i = next;
    }
    if in_bold {
        // Unbalanced — close to keep terminals sane.
        out.push_str(ANSI_BOLD_CLOSE);
    }
    out
}

/// Tests pin the pure renderer; cli main.rs sets `tty` via
/// `is_terminal::IsTerminal::is_terminal(&std::io::stdout())`.
pub fn stdout_is_tty() -> bool {
    is_terminal::IsTerminal::is_terminal(&std::io::stdout())
}
