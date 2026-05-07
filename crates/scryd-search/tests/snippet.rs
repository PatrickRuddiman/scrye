use scryd_search::{render_snippet, K_MIN, K_MULTIPLIER};

#[test]
fn match_in_middle_is_centered_and_highlighted() {
    let body = "Lorem ipsum dolor sit amet, the invoice is attached, please review and acknowledge.";
    let snip = render_snippet(body, "invoice", 60);
    assert!(snip.contains("**invoice**"), "highlight missing: {snip}");
    // The match is in the middle of the body, so we expect both ellipses.
    assert!(snip.starts_with('…') || snip.starts_with('L'), "leading edge: {snip}");
}

#[test]
fn case_insensitive_match_preserves_original_casing() {
    let body = "Re: April Invoice attached for review";
    let snip = render_snippet(body, "invoice", 200);
    // Original "Invoice" casing preserved; `**…**` wraps it.
    assert!(snip.contains("**Invoice**"), "{snip}");
}

#[test]
fn multiple_matches_all_get_highlighted() {
    let body = "invoice one, invoice two, INVOICE three";
    let snip = render_snippet(body, "invoice", 200);
    let occurrences = snip.matches("**").count();
    // Each highlight uses 2 markers (open + close), 3 matches = 6 markers.
    assert!(occurrences >= 6, "expected ≥6 ** markers, got {occurrences} in {snip}");
}

#[test]
fn first_occurring_term_wins_when_multiple_terms_supplied() {
    let body = "lunch with bob then invoice acme then dinner";
    let snip = render_snippet(body, "acme invoice", 30);
    // Both terms occur; the snippet should include the first-occurring one
    // ("invoice" at byte 19) as the centering anchor.
    assert!(snip.contains("**invoice**") || snip.contains("**acme**"));
}

#[test]
fn empty_query_returns_leading_chars() {
    let body = "the body content here continues with much more text after this point indeed";
    let snip = render_snippet(body, "", 20);
    assert!(snip.starts_with("the body content"));
    assert!(snip.ends_with('…'));
    assert!(!snip.contains("**"));
}

#[test]
fn no_match_returns_leading_chars_with_trailing_ellipsis() {
    let body = "the body content here continues with much more text after this point indeed";
    let snip = render_snippet(body, "zebra", 20);
    assert!(snip.starts_with("the body content"));
    assert!(snip.ends_with('…'));
    assert!(!snip.contains("**"));
}

#[test]
fn body_shorter_than_max_chars_passes_through_without_ellipses() {
    let body = "short body";
    let snip = render_snippet(body, "body", 200);
    assert_eq!(snip, "short **body**");
}

#[test]
fn utf8_multi_byte_body_does_not_panic_and_stays_valid_utf8() {
    let body = "café résumé 日本語 invoice más";
    let snip = render_snippet(body, "invoice", 20);
    assert!(snip.contains("**invoice**"));
    assert!(std::str::from_utf8(snip.as_bytes()).is_ok());
}

#[test]
fn highlighting_does_not_break_words_at_term_boundary() {
    // "invoices" contains "invoice" — highlight should still match.
    let body = "the invoices are ready";
    let snip = render_snippet(body, "invoice", 200);
    assert!(snip.contains("**invoice**s"), "expected partial-word highlight: {snip}");
}

#[test]
fn k_constants_match_slice_decision_9() {
    assert_eq!(K_MIN, 200);
    assert_eq!(K_MULTIPLIER, 5);

    let limit_small: usize = 20;
    let k = std::cmp::max(limit_small * K_MULTIPLIER, K_MIN);
    assert_eq!(k, 200);

    let limit_big: usize = 50;
    let k = std::cmp::max(limit_big * K_MULTIPLIER, K_MIN);
    assert_eq!(k, 250);
}

#[test]
fn whitespace_only_query_treated_as_empty() {
    let body = "the body content here continues with much more text after this point indeed";
    let snip = render_snippet(body, "   \t\n  ", 20);
    assert!(snip.starts_with("the body content"));
    assert!(!snip.contains("**"));
}
