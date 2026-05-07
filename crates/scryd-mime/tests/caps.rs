use scryd_mime::{parse, ParseContext, ParseFault, ParseOutcome, MAX_PARTS, MAX_PART_DEPTH};

fn ctx() -> ParseContext<'static> {
    ParseContext {
        account_id: "primary",
        folder: "INBOX",
        server_uid: 1,
        internal_date: None,
    }
}

fn make_nested_multipart(depth: usize) -> Vec<u8> {
    // Build N nested multipart/mixed wrappers, each enclosing the next, with
    // a final text/plain leaf at the deepest level.
    let mut header = String::from(
        "From: deep@x\r\nTo: bob@x\r\nSubject: deep\r\nDate: Mon, 12 Jan 2026 09:00:00 +0000\r\nMIME-Version: 1.0\r\n",
    );
    header.push_str(&format!(
        "Content-Type: multipart/mixed; boundary=\"b1\"\r\n\r\n"
    ));

    let mut s = String::new();
    // Build innermost first.
    let mut inner = String::from(
        "Content-Type: text/plain; charset=utf-8\r\n\r\nleaf body content here\r\n",
    );
    for level in (1..depth).rev() {
        let outer = level;
        let inner_b = level + 1;
        inner = format!(
            "Content-Type: multipart/mixed; boundary=\"b{inner_b}\"\r\n\r\n--b{inner_b}\r\n{inner}\r\n--b{inner_b}--\r\n"
        );
        let _ = outer; // numbered for readability
    }
    s.push_str("--b1\r\n");
    s.push_str(&inner);
    s.push_str("\r\n--b1--\r\n");

    let mut out = header.into_bytes();
    out.extend_from_slice(s.as_bytes());
    out
}

fn make_flat_multipart(part_count: usize) -> Vec<u8> {
    let mut s = String::from(
        "From: many@x\r\nTo: bob@x\r\nSubject: many\r\nDate: Mon, 12 Jan 2026 09:00:00 +0000\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=\"b\"\r\n\r\n",
    );
    for _ in 0..part_count {
        s.push_str("--b\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nx\r\n");
    }
    s.push_str("--b--\r\n");
    s.into_bytes()
}

#[test]
fn deeply_nested_multipart_is_unparseable_with_part_depth_cap() {
    let bytes = make_nested_multipart(MAX_PART_DEPTH + 2);
    match parse(&bytes, ctx()) {
        ParseOutcome::Unparseable(faults) => {
            assert!(
                faults.iter().any(|f| matches!(
                    f,
                    ParseFault::ParseCapExceeded { cap: "part-depth" }
                )),
                "expected part-depth cap, got {faults:?}"
            );
        }
        other => panic!("expected Unparseable, got {other:?}"),
    }
}

#[test]
fn multipart_with_too_many_parts_is_unparseable_with_part_count_cap() {
    let bytes = make_flat_multipart(MAX_PARTS + 5);
    match parse(&bytes, ctx()) {
        ParseOutcome::Unparseable(faults) => {
            assert!(
                faults.iter().any(|f| matches!(
                    f,
                    ParseFault::ParseCapExceeded { cap: "part-count" }
                )),
                "expected part-count cap, got {faults:?}"
            );
        }
        other => panic!("expected Unparseable, got {other:?}"),
    }
}

#[test]
fn well_shaped_multipart_passes_caps() {
    let bytes = make_flat_multipart(5);
    match parse(&bytes, ctx()) {
        ParseOutcome::Parsed(_) | ParseOutcome::ParsedDegraded(_, _) => {}
        ParseOutcome::Unparseable(faults) => {
            panic!("well-shaped message should not exceed caps: {faults:?}")
        }
    }
}
