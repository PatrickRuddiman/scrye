//! Thin wrapper over `async_imap::Session` that routes every wire emission
//! through the [`crate::verbs::assert_verb_allowed`] guard.
//!
//! Generic over the underlying byte stream so production (TLS) and tests
//! (in-process duplex pipes) can share the implementation.

use async_imap::Session;
use futures::{AsyncRead, AsyncWrite};

use crate::capabilities::Capabilities;
use crate::verbs::{assert_verb_allowed, ImapVerb};
use crate::ClientError;

/// Subset of `Mailbox` the fetch loops care about. `exists` is the
/// total message count; `uid_validity` and `uid_next` come straight
/// from the EXAMINE response.
#[derive(Debug, Clone, Copy)]
pub struct MailboxMeta {
    pub exists: u32,
    pub uid_validity: Option<u32>,
    pub uid_next: Option<u32>,
}

pub struct Client<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + std::fmt::Debug,
{
    session: Session<S>,
}

impl<S> Client<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + std::fmt::Debug,
{
    /// Construct a client around an already-authenticated `Session`.
    /// The [`crate::login`] helper produces one of these.
    pub fn from_session(session: Session<S>) -> Self {
        Self { session }
    }

    /// Crate-internal access to the underlying session — needed by
    /// `crate::fetch::fetch_batch` to consume the FETCH stream and
    /// produce parsed [`crate::sink::FetchedMessage`] values.
    pub(crate) fn session_mut(&mut self) -> &mut Session<S> {
        &mut self.session
    }

    /// Open a folder in EXAMINE (read-only) mode.
    pub async fn examine(&mut self, folder: &str) -> Result<(), ClientError> {
        assert_verb_allowed(&ImapVerb::Examine)?;
        self.session
            .examine(folder)
            .await
            .map(|_| ())
            .map_err(|e| ClientError::Server(e.to_string()))
    }

    /// EXAMINE that returns the mailbox metadata (UIDVALIDITY, UIDNEXT,
    /// EXISTS) the fetch loops need to drive backfill / incremental
    /// state machines.
    pub async fn examine_meta(&mut self, folder: &str) -> Result<MailboxMeta, ClientError> {
        assert_verb_allowed(&ImapVerb::Examine)?;
        let mb = self
            .session
            .examine(folder)
            .await
            .map_err(|e| ClientError::Server(e.to_string()))?;
        Ok(MailboxMeta {
            exists: mb.exists,
            uid_validity: mb.uid_validity,
            uid_next: mb.uid_next,
        })
    }

    /// Run CAPABILITY and return the parsed flag set. async-imap exposes
    /// individual `Capability` values via Debug only; we iterate and parse
    /// from their Debug form to detect the IDLE flag rather than fight
    /// the upstream type.
    pub async fn capabilities(&mut self) -> Result<Capabilities, ClientError> {
        assert_verb_allowed(&ImapVerb::Capability)?;
        let caps = self
            .session
            .capabilities()
            .await
            .map_err(|e| ClientError::Server(e.to_string()))?;
        let mut tokens: Vec<String> = Vec::new();
        for c in caps.iter() {
            tokens.push(format!("{c:?}"));
        }
        let refs: Vec<&str> = tokens.iter().map(String::as_str).collect();
        Ok(Capabilities::from_tokens(refs))
    }

    /// `UID FETCH <range> (<items>)`. Returned stream lives inside the
    /// session; downstream tasks consume it.
    pub async fn uid_fetch(
        &mut self,
        range: &str,
        items: &str,
    ) -> Result<(), ClientError> {
        assert_verb_allowed(&ImapVerb::UidFetch)?;
        let mut stream = self
            .session
            .uid_fetch(range, items)
            .await
            .map_err(|e| ClientError::Server(e.to_string()))?;
        // task 13 consumes the stream; task 12 only validates the call.
        use futures::StreamExt;
        while stream.next().await.is_some() {}
        Ok(())
    }

    /// `UID SEARCH <criteria>`. Returns the match-set as a Vec<u32>.
    pub async fn uid_search(&mut self, criteria: &str) -> Result<Vec<u32>, ClientError> {
        assert_verb_allowed(&ImapVerb::UidSearch)?;
        let set = self
            .session
            .uid_search(criteria)
            .await
            .map_err(|e| ClientError::Server(e.to_string()))?;
        Ok(set.into_iter().collect())
    }

    /// Send a plain NOOP. Useful in test paths where we want to confirm the
    /// connection is alive without changing state.
    pub async fn noop(&mut self) -> Result<(), ClientError> {
        assert_verb_allowed(&ImapVerb::Noop)?;
        self.session
            .noop()
            .await
            .map_err(|e| ClientError::Server(e.to_string()))
    }

    /// Run an IDLE handle. The returned object owns the session for the
    /// idle lifetime; task 14 layers the EXISTS-detect / DONE / recycle
    /// loop on top of it.
    pub async fn idle_start(self) -> Result<async_imap::extensions::idle::Handle<S>, ClientError> {
        assert_verb_allowed(&ImapVerb::Idle)?;
        let mut handle = self.session.idle();
        handle
            .init()
            .await
            .map_err(|e| ClientError::Server(e.to_string()))?;
        Ok(handle)
    }

    /// Cooperative LOGOUT.
    pub async fn logout(&mut self) -> Result<(), ClientError> {
        assert_verb_allowed(&ImapVerb::Logout)?;
        self.session
            .logout()
            .await
            .map_err(|e| ClientError::Server(e.to_string()))
    }
}
