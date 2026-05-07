use scryd_imap::Capabilities;

#[test]
fn parses_idle_from_typical_capability_response() {
    let caps = Capabilities::from_line("CAPABILITY IMAP4rev1 IDLE STARTTLS LITERAL+");
    assert!(caps.idle);
}

#[test]
fn idle_match_is_case_insensitive() {
    let caps = Capabilities::from_line("IMAP4rev1 idle");
    assert!(caps.idle);
}

#[test]
fn missing_idle_yields_false() {
    let caps = Capabilities::from_line("IMAP4rev1 STARTTLS");
    assert!(!caps.idle);
}

#[test]
fn empty_response_yields_default() {
    let caps = Capabilities::from_line("");
    assert!(!caps.idle);
}

#[test]
fn from_tokens_strips_capability_tag() {
    let caps = Capabilities::from_tokens(["CAPABILITY", "IDLE"]);
    assert!(caps.idle);
}

#[test]
fn extra_whitespace_is_handled() {
    let caps = Capabilities::from_line("  IMAP4rev1   IDLE   ");
    assert!(caps.idle);
}
