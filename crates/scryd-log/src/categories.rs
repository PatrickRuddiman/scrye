//! Closed-set strings for tagging log events. Both modules are kept exhaustive;
//! adding a new variant requires touching this file plus a slice update.

/// Failure-category strings, matching the spec's §4 closed set verbatim.
pub mod category {
    pub const CONNECT_FAILURE: &str = "connect failure";
    pub const TLS_FAILURE: &str = "tls failure";
    pub const AUTH_REJECTION: &str = "auth rejection";
    pub const PUSH_CHANNEL_DROP: &str = "push-channel drop";
    pub const UID_VALIDITY_RESET: &str = "uid-validity reset triggering re-sync";
    pub const SINGLE_MESSAGE_PARSE_FAILURE: &str = "single-message parse failure";
    pub const SINGLE_MESSAGE_FT_INDEXER_FAILURE: &str = "single-message full-text indexer failure";
    pub const SINGLE_MESSAGE_SEMANTIC_INDEXER_FAILURE: &str = "single-message semantic indexer failure";
    pub const DISK_FULL: &str = "disk-full";
    pub const CONFIG_PARSE_ERROR: &str = "configuration parse error";
    pub const CONFIG_PERMISSION_ERROR: &str = "configuration permission error";
    pub const NON_OWNER_REJECTION: &str = "non-owner-user connection rejection";
}

/// Discriminator strings for non-failure lifecycle events.
pub mod kind {
    pub const STARTUP: &str = "startup";
    pub const SHUTDOWN: &str = "shutdown";
    pub const SYNC_PASS_START: &str = "sync_pass_start";
    pub const SYNC_PASS_COMPLETE: &str = "sync_pass_complete";
    pub const BACKFILL_PROGRESS: &str = "backfill_progress";
    pub const IDLE_ENTER: &str = "idle_enter";
    pub const IDLE_DROP: &str = "idle_drop";
    pub const IDLE_RECYCLE: &str = "idle_recycle";
    pub const UIDVALIDITY_RESET: &str = "uidvalidity_reset";
    pub const REINDEX_START: &str = "reindex_start";
    pub const REINDEX_COMPLETE: &str = "reindex_complete";
    pub const ACCOUNT_RECONCILED: &str = "account_reconciled";
    pub const REQUEST: &str = "request";
}
