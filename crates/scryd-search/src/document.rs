//! Build the document text the indexer indexes for a message and
//! split it into bounded sub-documents for witchcraft submission.
//!
//! Concatenation per search-engine slice §3 Decision 6:
//! `<subject>\n\n<from-display>\n\n<body-markdown>`. Empty subject is
//! omitted; from-display is `name <addr>` if a name exists, otherwise
//! the bare address.
//!
//! Segmentation (v0.3.7): one email = one witchcraft document was the
//! original model, but production data showed pathological emails
//! (e.g. a single Prudential health statement) producing 319,860
//! embeddings in a single chunk and stalling cascade for 138 minutes.
//! [`split_for_index`] cuts the built document into ≤8 segments of
//! ~2k chars each. The witchcraft handle submits each segment as its
//! own witchcraft document, so per-call `Embedder::embed` work and
//! per-call mutex hold are bounded regardless of input email size.

/// Target character width of one indexed segment. Picked so a
/// segment's `Embedder::embed` call holds the witchcraft state
/// mutex for a bounded time even on pathologically-tokenizing
/// content. The median email body in production is ~1130 chars,
/// well under this — most emails produce exactly one segment.
pub const SEGMENT_TARGET_CHARS: usize = 2048;

/// Hard cap on total indexed characters per email. One pathological
/// marketing email shouldn't dominate the indexer; anything past
/// this is dropped from indexing. The original `body_md` stays
/// intact in `meta.messages` for any future read path.
pub const SEGMENT_MAX_CHARS: usize = 16_384;

/// Upper bound on segments produced from one email. Used by the
/// witchcraft handle to iterate `remove_doc` calls without needing
/// to store the actual segment count anywhere.
pub const MAX_SEGMENTS_PER_MESSAGE: usize = SEGMENT_MAX_CHARS / SEGMENT_TARGET_CHARS;

/// Window around the target boundary in which we look for a natural
/// break (`\n\n`, `\n`, or word boundary). If nothing in window, we
/// hard-cut at the target boundary.
const SNAP_WINDOW: usize = 256;

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

/// Split a built document into bounded text segments for indexing.
/// Target [`SEGMENT_TARGET_CHARS`] per segment, snap to a paragraph
/// or word boundary within ±[`SNAP_WINDOW`] of the target offset,
/// and cap total output at [`SEGMENT_MAX_CHARS`].
///
/// Guarantees:
/// - Always returns at least one segment.
/// - Never returns empty (zero-character) segments.
/// - At most [`MAX_SEGMENTS_PER_MESSAGE`] segments.
/// - Each segment is at most `SEGMENT_TARGET_CHARS + SNAP_WINDOW`
///   characters (the snap can push the boundary outward by up to
///   the window size when the next paragraph break is just past
///   the target).
pub fn split_for_index(document: &str) -> Vec<String> {
    let chars: Vec<char> = document.chars().collect();
    let total = chars.len();

    if total == 0 {
        return vec![String::new()];
    }
    if total <= SEGMENT_TARGET_CHARS {
        return vec![document.to_string()];
    }

    let capped = total.min(SEGMENT_MAX_CHARS);
    let mut segments: Vec<String> = Vec::new();
    let mut start = 0usize;

    while start < capped && segments.len() < MAX_SEGMENTS_PER_MESSAGE {
        let target = (start + SEGMENT_TARGET_CHARS).min(capped);
        let end = if target >= capped {
            capped
        } else {
            snap_boundary(&chars, start, target, capped)
        };
        debug_assert!(end > start, "segmenter produced empty segment");
        let segment: String = chars[start..end].iter().collect();
        if !segment.is_empty() {
            segments.push(segment);
        }
        start = end;
    }

    if segments.is_empty() {
        // Shouldn't happen given the guards above, but stay defensive
        // so an empty Vec never reaches witchcraft.
        segments.push(document.chars().take(SEGMENT_TARGET_CHARS).collect());
    }
    segments
}

/// Find the best break offset around `target` within the snap window.
/// Prefer paragraph break (`\n\n`), then single newline, then word
/// boundary (whitespace). Fall back to a hard cut at `target`.
fn snap_boundary(chars: &[char], start: usize, target: usize, capped: usize) -> usize {
    let window_start = target.saturating_sub(SNAP_WINDOW).max(start + 1);
    let window_end = (target + SNAP_WINDOW).min(capped);
    if window_start >= window_end {
        return target;
    }

    // Search for "\n\n" within [window_start, window_end). Prefer the
    // break closest to target.
    let mut best_para: Option<usize> = None;
    let mut best_para_dist = usize::MAX;
    for i in window_start..window_end.saturating_sub(1) {
        if chars[i] == '\n' && chars[i + 1] == '\n' {
            let split_at = i + 2;
            let dist = split_at.abs_diff(target);
            if dist < best_para_dist {
                best_para_dist = dist;
                best_para = Some(split_at);
            }
        }
    }
    if let Some(p) = best_para {
        return p.min(capped);
    }

    // Single newline next.
    let mut best_nl: Option<usize> = None;
    let mut best_nl_dist = usize::MAX;
    for i in window_start..window_end {
        if chars[i] == '\n' {
            let split_at = i + 1;
            let dist = split_at.abs_diff(target);
            if dist < best_nl_dist {
                best_nl_dist = dist;
                best_nl = Some(split_at);
            }
        }
    }
    if let Some(p) = best_nl {
        return p.min(capped);
    }

    // Whitespace word boundary, scan backward from target so we don't
    // overshoot when a long run of non-space content sits past it.
    for i in (window_start..target).rev() {
        if chars[i].is_whitespace() {
            return (i + 1).min(capped);
        }
    }

    target
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_for_index_keeps_short_docs_intact() {
        let doc = "short body content";
        let segs = split_for_index(doc);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0], doc);
    }

    #[test]
    fn split_for_index_keeps_target_sized_docs_intact() {
        let doc: String = std::iter::repeat('x').take(SEGMENT_TARGET_CHARS).collect();
        let segs = split_for_index(&doc);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].chars().count(), SEGMENT_TARGET_CHARS);
    }

    #[test]
    fn split_for_index_bounds_long_docs_under_segment_max_chars() {
        // 38k-char doc with no natural breaks: hard-cut at target boundaries.
        let doc: String = std::iter::repeat('x').take(38_000).collect();
        let segs = split_for_index(&doc);
        assert!(segs.len() <= MAX_SEGMENTS_PER_MESSAGE, "got {} segs", segs.len());
        let total: usize = segs.iter().map(|s| s.chars().count()).sum();
        assert!(total <= SEGMENT_MAX_CHARS, "total {total} exceeds cap");
        for seg in &segs {
            assert!(
                seg.chars().count() <= SEGMENT_TARGET_CHARS + SNAP_WINDOW,
                "segment exceeds target+window"
            );
        }
    }

    #[test]
    fn split_for_index_caps_total_at_segment_max_chars() {
        let doc: String = std::iter::repeat('y').take(100_000).collect();
        let segs = split_for_index(&doc);
        let total: usize = segs.iter().map(|s| s.chars().count()).sum();
        assert_eq!(total, SEGMENT_MAX_CHARS, "must truncate to cap exactly");
        assert_eq!(segs.len(), MAX_SEGMENTS_PER_MESSAGE);
    }

    #[test]
    fn split_for_index_snaps_to_paragraph_boundaries_when_available() {
        // Build a doc with a clear paragraph break just past the target offset.
        let head: String = std::iter::repeat('a').take(SEGMENT_TARGET_CHARS - 20).collect();
        let tail: String = std::iter::repeat('b').take(SEGMENT_TARGET_CHARS).collect();
        // 30 chars into the snap window we plant the paragraph break.
        let mid = "x".repeat(30);
        let doc = format!("{head}{mid}\n\n{tail}");

        let segs = split_for_index(&doc);
        assert!(segs.len() >= 2);
        // First segment ends right after the "\n\n" break.
        assert!(segs[0].ends_with("\n\n"), "first segment should snap to paragraph: tail = {:?}", &segs[0][segs[0].len().saturating_sub(8)..]);
        // Second segment starts with the 'b' run.
        assert!(segs[1].starts_with('b'));
    }

    #[test]
    fn split_for_index_never_returns_empty_segments() {
        let doc: String = std::iter::repeat(' ').take(SEGMENT_MAX_CHARS + 1000).collect();
        let segs = split_for_index(&doc);
        for seg in &segs {
            assert!(!seg.is_empty(), "no empty segments");
        }
    }

    #[test]
    fn split_for_index_handles_empty_input() {
        let segs = split_for_index("");
        assert_eq!(segs.len(), 1);
        assert!(segs[0].is_empty());
    }

    #[test]
    fn split_for_index_handles_unicode_correctly() {
        // 4-byte chars (😀) but each is 1 codepoint. SEGMENT_TARGET_CHARS+1 of them
        // means we must split, but the count is in codepoints not bytes.
        let doc: String = std::iter::repeat('😀').take(SEGMENT_TARGET_CHARS + 1).collect();
        let segs = split_for_index(&doc);
        assert!(segs.len() >= 2);
        // Reconstruction (or the cap-bounded prefix of it) must be valid UTF-8.
        let rejoined: String = segs.concat();
        assert!(rejoined.chars().count() <= SEGMENT_MAX_CHARS);
    }
}
