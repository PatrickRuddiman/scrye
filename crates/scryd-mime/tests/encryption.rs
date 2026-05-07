use scryd_mime::{parse, ParseContext, ParseFault, ParseOutcome};

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

#[test]
fn pgp_encrypted_yields_placeholder_body_and_fault() {
    let bytes = load("pgp-encrypted.eml");
    let (m, faults) = match parse(&bytes, ctx()) {
        ParseOutcome::ParsedDegraded(m, f) => (m, f),
        other => panic!("expected ParsedDegraded for PGP, got {other:?}"),
    };
    assert!(m.body_md.contains("[encrypted message — not indexed by scryd]"));
    assert!(m.attachments.is_empty(), "PGP body should yield no attachments");
    assert!(
        faults.iter().any(|f| matches!(f, ParseFault::EncryptedNotIndexed)),
        "EncryptedNotIndexed fault expected: {faults:?}"
    );

    // Headers still get extracted so the message is queryable by date/from.
    assert_eq!(m.subject.as_deref(), Some("secret"));
    assert_eq!(m.from.addr, "alice@example.com");
}

#[test]
fn smime_signed_parses_inner_body_normally() {
    let bytes = load("smime-signed.eml");
    let (m, _faults) = match parse(&bytes, ctx()) {
        ParseOutcome::Parsed(m) => (m, vec![]),
        ParseOutcome::ParsedDegraded(m, f) => (m, f),
        other => panic!("expected ok outcome for S/MIME, got {other:?}"),
    };
    // The signed inner part's body should land in body_md.
    assert!(
        m.body_md.contains("This is the signed body inside an S/MIME envelope"),
        "inner body missing: {}",
        m.body_md
    );
    assert!(
        !m.body_md.contains("[encrypted message"),
        "S/MIME signed must not be treated as encrypted"
    );
    // The signature blob lands in the attachments list.
    let sig = m
        .attachments
        .iter()
        .find(|a| a.filename == "smime.p7s")
        .expect("signature attachment present");
    assert_eq!(sig.mime_type, "application/pkcs7-signature");
}
