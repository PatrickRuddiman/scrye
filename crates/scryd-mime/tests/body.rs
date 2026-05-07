use scryd_mime::{parse, ParseContext, ParseFault, ParseOutcome, BODY_MAX_BYTES};

const FIXTURES: &str = "tests/fixtures";

fn ctx() -> ParseContext<'static> {
    ParseContext {
        account_id: "primary",
        folder: "INBOX",
        server_uid: 1,
        internal_date: None,
    }
}

fn load(name: &str) -> Vec<u8> {
    std::fs::read(format!("{FIXTURES}/{name}")).expect("fixture exists")
}

fn parsed(outcome: ParseOutcome) -> scryd_mime::ParsedMessage {
    match outcome {
        ParseOutcome::Parsed(m) => m,
        ParseOutcome::ParsedDegraded(m, _) => m,
        ParseOutcome::Unparseable(faults) => panic!("unparseable: {faults:?}"),
    }
}

#[test]
fn plain_only_returns_plaintext_body_verbatim() {
    let bytes = load("plain-only.eml");
    let m = parsed(parse(&bytes, ctx()));
    assert_eq!(m.subject.as_deref(), Some("Plain only"));
    assert_eq!(m.from.addr, "alice@example.com");
    assert!(m.body_md.contains("This is a plaintext-only message"));
    assert!(!m.body_md.contains("<"), "plaintext should not contain HTML");
}

#[test]
fn html_only_converts_to_markdown_after_pre_clean() {
    let bytes = load("html-only.eml");
    let m = parsed(parse(&bytes, ctx()));

    // Markdown rendering preserves the visible content...
    assert!(m.body_md.contains("headline"));
    assert!(m.body_md.contains("example"));

    // ...but the pre-clean strips the noise.
    assert!(!m.body_md.contains("<script>"));
    assert!(!m.body_md.contains("tracker("));
    assert!(!m.body_md.contains("color: red"), "style block leaked");
    assert!(!m.body_md.contains("color:#ccc"), "inline style leaked");
    assert!(!m.body_md.contains("a comment"), "html comment leaked");
    assert!(!m.body_md.contains("pixel.gif"), "1x1 tracking pixel leaked");
}

#[test]
fn multipart_prefers_plain_when_above_threshold() {
    let bytes = load("multipart-alt-plain-wins.eml");
    let m = parsed(parse(&bytes, ctx()));
    assert!(m.body_md.contains("plaintext branch is well over one hundred"));
    assert!(
        !m.body_md.contains("ignored"),
        "html branch text leaked into body"
    );
}

#[test]
fn multipart_falls_back_to_html_when_plain_is_a_stub() {
    let bytes = load("multipart-alt-html-wins.eml");
    let m = parsed(parse(&bytes, ctx()));
    assert!(m.body_md.contains("Big news"));
    assert!(m.body_md.contains("real content"));
    assert!(
        !m.body_md.contains("View in browser"),
        "stub plaintext should be ignored"
    );
}

#[test]
fn body_over_one_megabyte_is_truncated_with_sentinel() {
    let mut bytes = Vec::with_capacity(2 * BODY_MAX_BYTES);
    bytes.extend_from_slice(
        b"From: Loud <loud@example.com>\r\n\
          To: Bob <bob@example.com>\r\n\
          Subject: huge\r\n\
          Date: Mon, 12 Jan 2026 09:00:00 +0000\r\n\
          Message-ID: <huge@example.com>\r\n\
          Content-Type: text/plain; charset=utf-8\r\n\r\n",
    );
    bytes.extend(std::iter::repeat(b'A').take(2 * BODY_MAX_BYTES));

    let outcome = parse(&bytes, ctx());
    let (m, faults) = match outcome {
        ParseOutcome::ParsedDegraded(m, f) => (m, f),
        other => panic!("expected ParsedDegraded for oversized body, got {other:?}"),
    };

    assert!(m.body_md.len() <= BODY_MAX_BYTES);
    assert!(m.body_md.ends_with("_[truncated by scryd at 1MB]_"));

    let truncated = faults
        .iter()
        .find(|f| matches!(f, ParseFault::BodyTruncated { .. }));
    assert!(truncated.is_some(), "BodyTruncated fault must be present");
}

#[test]
fn missing_date_falls_back_to_internaldate() {
    let bytes = b"From: a <a@x>\r\nSubject: no-date\r\nContent-Type: text/plain\r\n\r\nbody";
    let mut c = ctx();
    c.internal_date = Some(1_700_000_000);
    let outcome = parse(bytes, c);
    let (m, faults) = match outcome {
        ParseOutcome::ParsedDegraded(m, f) => (m, f),
        other => panic!("expected ParsedDegraded, got {other:?}"),
    };
    assert_eq!(m.date_unix, 1_700_000_000);
    assert!(faults
        .iter()
        .any(|f| matches!(f, ParseFault::DateFallback { source: "internaldate" })));
}

#[test]
fn outer_parse_failure_returns_unparseable() {
    // mail-parser is permissive; feed it pure binary noise so it can't recover.
    let bytes = vec![0u8; 4];
    match parse(&bytes, ctx()) {
        ParseOutcome::Unparseable(_) | ParseOutcome::ParsedDegraded(_, _) => {}
        ParseOutcome::Parsed(m) => panic!("expected fault outcome, got {m:?}"),
    }
}
