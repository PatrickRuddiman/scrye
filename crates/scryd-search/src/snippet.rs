//! Snippet generator for the api slice's full-text and hybrid responses.
//!
//! Finds the first occurrence of any whitespace-split query term (ASCII
//! case-insensitive), returns roughly `max_chars` worth of body centered on
//! the match, with `…` ellipsis at non-start / non-end edges, and every
//! occurring term wrapped in `**…**` for Markdown-bold rendering. If no
//! term occurs verbatim, returns the leading `max_chars` of the body.
//!
//! Case folding is intentionally ASCII-only — non-ASCII matching uses
//! exact byte equality so the byte offsets stay aligned with the original
//! body string.

/// Build a snippet for `body_md` highlighting any occurrences of terms in
/// the whitespace-split query `q`. Honors UTF-8 character boundaries.
pub fn render(body_md: &str, q: &str, max_chars: usize) -> String {
    let terms: Vec<&str> = q.split_whitespace().filter(|t| !t.is_empty()).collect();

    let first_match = if terms.is_empty() {
        None
    } else {
        find_first_match(body_md, &terms)
    };

    let (slice_start, slice_end) = match first_match {
        Some(pos) => window_around(body_md, pos, max_chars),
        None => leading_window(body_md, max_chars),
    };

    let mut out = String::with_capacity(slice_end - slice_start + 8);
    if slice_start > 0 {
        out.push('…');
    }
    let raw = &body_md[slice_start..slice_end];
    if terms.is_empty() {
        out.push_str(raw);
    } else {
        push_highlighted(&mut out, raw, &terms);
    }
    if slice_end < body_md.len() {
        out.push('…');
    }
    out
}

fn find_first_match(body: &str, terms: &[&str]) -> Option<usize> {
    let mut earliest: Option<usize> = None;
    for term in terms {
        let tl = term.len();
        if tl == 0 || tl > body.len() {
            continue;
        }
        let mut i = 0;
        while i + tl <= body.len() {
            if !body.is_char_boundary(i) {
                i += 1;
                continue;
            }
            if !body.is_char_boundary(i + tl) {
                i += 1;
                continue;
            }
            if body[i..i + tl].eq_ignore_ascii_case(term) {
                earliest = match earliest {
                    None => Some(i),
                    Some(e) if i < e => Some(i),
                    Some(e) => Some(e),
                };
                break;
            }
            i += 1;
        }
    }
    earliest
}

fn window_around(body: &str, pos: usize, max_chars: usize) -> (usize, usize) {
    let half = max_chars / 2;
    let mut s = pos.saturating_sub(half);
    let mut e = (pos + half).min(body.len());
    while s > 0 && !body.is_char_boundary(s) {
        s -= 1;
    }
    while e < body.len() && !body.is_char_boundary(e) {
        e += 1;
    }
    (s, e)
}

fn leading_window(body: &str, max_chars: usize) -> (usize, usize) {
    let mut e = max_chars.min(body.len());
    while e < body.len() && !body.is_char_boundary(e) {
        e += 1;
    }
    (0, e)
}

fn push_highlighted(out: &mut String, slice: &str, terms: &[&str]) {
    let mut i = 0;
    while i < slice.len() {
        let mut matched_len: Option<usize> = None;
        for term in terms {
            let tl = term.len();
            if tl == 0 || i + tl > slice.len() {
                continue;
            }
            if !slice.is_char_boundary(i) || !slice.is_char_boundary(i + tl) {
                continue;
            }
            if slice[i..i + tl].eq_ignore_ascii_case(term) {
                matched_len = Some(tl);
                break;
            }
        }
        if let Some(len) = matched_len {
            out.push_str("**");
            out.push_str(&slice[i..i + len]);
            out.push_str("**");
            i += len;
        } else {
            let next = next_char_boundary(slice, i);
            out.push_str(&slice[i..next]);
            i = next;
        }
    }
}

fn next_char_boundary(s: &str, i: usize) -> usize {
    let mut j = i + 1;
    while j < s.len() && !s.is_char_boundary(j) {
        j += 1;
    }
    j.min(s.len())
}
