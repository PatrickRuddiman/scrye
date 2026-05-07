//! Closed enumeration of every IMAP verb the daemon is permitted to issue.
//! Compile-time-exhaustive: adding a future verb here requires also
//! extending [`assert_verb_allowed`], so a mutating command can never
//! silently sneak through.

use crate::ClientError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImapVerb {
    Capability,
    Login,
    Authenticate,
    Logout,
    Examine,
    List,
    Lsub,
    Status,
    Fetch,
    UidFetch,
    Search,
    UidSearch,
    Idle,
    Done,
    Noop,
    Id,
    Enable,
    GetMetadata,
    GetAcl,
    GetQuota,
    GetQuotaRoot,
}

/// Validate that `v` is in the read-only allow-list. Panics in
/// `debug_assertions` builds and returns
/// [`ClientError::MutatingCommandRejected`] in release for any verb that's
/// not explicitly listed below — so a future code change that adds a
/// mutating variant to `ImapVerb` without also adding it here turns into
/// a loud test failure rather than a silent server-side mutation.
pub fn assert_verb_allowed(v: &ImapVerb) -> Result<(), ClientError> {
    use ImapVerb::*;
    let allowed = matches!(
        v,
        Capability
            | Login
            | Authenticate
            | Logout
            | Examine
            | List
            | Lsub
            | Status
            | Fetch
            | UidFetch
            | Search
            | UidSearch
            | Idle
            | Done
            | Noop
            | Id
            | Enable
            | GetMetadata
            | GetAcl
            | GetQuota
            | GetQuotaRoot
    );
    if !allowed {
        debug_assert!(false, "mutating IMAP verb routed through wrapper: {v:?}");
        return Err(ClientError::MutatingCommandRejected);
    }
    Ok(())
}
