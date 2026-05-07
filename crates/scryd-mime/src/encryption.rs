//! Classify the top-level container as Plain, PgpEncrypted, or SmimeSigned.
//! PGP-encrypted messages get a placeholder body and no attachments; signed
//! messages parse normally because mail-parser already flattens the
//! signature-wrapper part.

use mail_parser::{Message, MimeHeaders};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncryptionClass {
    Plain,
    PgpEncrypted,
    SmimeSigned,
}

pub const ENCRYPTED_PLACEHOLDER: &str = "[encrypted message — not indexed by scryd]";

pub fn classify(message: &Message<'_>) -> EncryptionClass {
    let Some(ct) = message.content_type() else {
        return EncryptionClass::Plain;
    };
    let main = ct.ctype();
    let sub = ct.subtype().unwrap_or("");
    if !main.eq_ignore_ascii_case("multipart") {
        return EncryptionClass::Plain;
    }
    if sub.eq_ignore_ascii_case("encrypted") {
        return EncryptionClass::PgpEncrypted;
    }
    if sub.eq_ignore_ascii_case("signed") {
        return EncryptionClass::SmimeSigned;
    }
    EncryptionClass::Plain
}
