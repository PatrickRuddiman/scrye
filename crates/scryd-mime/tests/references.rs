use scryd_mime::references::{normalize_message_id, normalize_references};

#[test]
fn strips_brackets_and_lowercases() {
    assert_eq!(normalize_message_id("<ABC@x>").as_deref(), Some("abc@x"));
    assert_eq!(normalize_message_id("DEF@X.COM").as_deref(), Some("def@x.com"));
}

#[test]
fn whitespace_is_trimmed() {
    assert_eq!(normalize_message_id("  <  a@x  >  ").as_deref(), Some("a@x"));
}

#[test]
fn rejects_empty_and_no_at_sign() {
    assert!(normalize_message_id("").is_none());
    assert!(normalize_message_id("<>").is_none());
    assert!(normalize_message_id("not-a-msg-id").is_none());
}

#[test]
fn references_split_and_normalize_each_token() {
    let v = normalize_references("<a@x> <B@y>");
    assert_eq!(v, vec!["a@x".to_string(), "b@y".to_string()]);
}

#[test]
fn references_drop_malformed_tokens_silently() {
    let v = normalize_references("<a@x> garbage <b@y>");
    assert_eq!(v, vec!["a@x".to_string(), "b@y".to_string()]);
}
