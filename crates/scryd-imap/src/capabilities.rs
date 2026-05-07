//! Parse server CAPABILITY responses for the flags the sync layer cares
//! about. Future fields (CONDSTORE, QRESYNC, etc.) are added as the
//! relevant code paths land.

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Capabilities {
    pub idle: bool,
}

impl Capabilities {
    /// Parse from the space-separated tokens of a CAPABILITY response —
    /// e.g. `["IMAP4rev1", "IDLE", "STARTTLS"]`. Tokens are matched
    /// case-insensitively per RFC 3501.
    pub fn from_tokens<'a, I>(tokens: I) -> Self
    where
        I: IntoIterator<Item = &'a str>,
    {
        let mut caps = Self::default();
        for tok in tokens {
            if tok.eq_ignore_ascii_case("IDLE") {
                caps.idle = true;
            }
        }
        caps
    }

    /// Convenience: split a raw CAPABILITY line on whitespace and parse.
    /// Strips a leading `CAPABILITY` token if present.
    pub fn from_line(line: &str) -> Self {
        let tokens = line
            .split_whitespace()
            .filter(|t| !t.eq_ignore_ascii_case("CAPABILITY"));
        Self::from_tokens(tokens)
    }
}
