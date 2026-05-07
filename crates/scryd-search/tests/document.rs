use scryd_search::build_document;

#[test]
fn full_set_includes_subject_name_addr_and_body() {
    let doc = build_document(
        Some("hello"),
        "alice@example.com",
        Some("Alice"),
        "body content",
    );
    assert!(doc.starts_with("hello"));
    assert!(doc.contains("Alice <alice@example.com>"));
    assert!(doc.ends_with("body content"));
}

#[test]
fn missing_subject_omits_subject_block() {
    let doc = build_document(None, "alice@example.com", Some("Alice"), "body");
    // Doc starts straight with from-display; no subject prefix.
    assert!(doc.starts_with("Alice <alice@example.com>\n\nbody"));
}

#[test]
fn empty_subject_string_omits_subject_block() {
    let doc = build_document(Some("   "), "alice@example.com", None, "body");
    assert!(doc.starts_with("alice@example.com"));
}

#[test]
fn missing_name_uses_bare_address() {
    let doc = build_document(Some("subj"), "alice@example.com", None, "body");
    assert!(doc.contains("subj\n\nalice@example.com\n\nbody"));
}

#[test]
fn empty_body_still_produces_well_formed_doc() {
    let doc = build_document(Some("subj"), "a@x", Some("A"), "");
    assert_eq!(doc, "subj\n\nA <a@x>\n\n");
}
