use scryd_mime::{parse, ParseContext, ParseFault, ParseOutcome, ParsedMessage};

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

fn parsed(outcome: ParseOutcome) -> (ParsedMessage, Vec<ParseFault>) {
    match outcome {
        ParseOutcome::Parsed(m) => (m, vec![]),
        ParseOutcome::ParsedDegraded(m, f) => (m, f),
        ParseOutcome::Unparseable(faults) => panic!("unparseable: {faults:?}"),
    }
}

#[test]
fn pdf_attachment_yields_metadata() {
    let bytes = load("with-pdf-attachment.eml");
    let (m, faults) = parsed(parse(&bytes, ctx()));
    assert_eq!(m.attachments.len(), 1);
    let a = &m.attachments[0];
    assert_eq!(a.filename, "invoice.pdf");
    assert_eq!(a.mime_type, "application/pdf");
    assert!(a.size_bytes > 0, "size_bytes recorded");
    assert!(faults.is_empty(), "no faults: {faults:?}");
}

#[test]
fn inline_image_listed_as_attachment_and_substituted_in_body() {
    let bytes = load("inline-cid-image.eml");
    let (m, _) = parsed(parse(&bytes, ctx()));

    // Inline image surfaces as an attachment for the metadata listing.
    let img = m.attachments.iter().find(|a| a.filename == "cat.png");
    assert!(
        img.is_some(),
        "inline image with filename should be in attachments: {:?}",
        m.attachments
    );

    // The body's <img src="cid:thecat"> is replaced with [image: cat.png];
    // htmd preserves bracketed plain text verbatim.
    assert!(
        m.body_md.contains("[image: cat.png]"),
        "expected cid placeholder, got: {}",
        m.body_md
    );
    assert!(!m.body_md.contains("cid:thecat"));
}

#[test]
fn rfc2047_encoded_filenames_decode() {
    let bytes = load("encoded-filename.eml");
    let (m, _) = parsed(parse(&bytes, ctx()));
    assert_eq!(m.attachments.len(), 1);
    let a = &m.attachments[0];
    // mail-parser decodes the encoded-word; the result should be
    // a normal UTF-8 string ending in .pdf — we don't pin the exact
    // glyphs because those are encoder-specific, just that decoding
    // ran (i.e. no =?...?= leftover).
    assert!(
        !a.filename.contains("=?UTF-8?"),
        "encoded-word leaked: {}",
        a.filename
    );
    assert!(a.filename.ends_with(".pdf"), "filename: {}", a.filename);
}

#[test]
fn root_part_never_listed_as_attachment() {
    let bytes = load("plain-only.eml");
    let (m, _) = parsed(parse(&bytes, ctx()));
    assert!(m.attachments.is_empty());
}
