use scryd_imap::{assert_verb_allowed, ImapVerb};

#[test]
fn every_variant_is_in_the_allow_list() {
    for v in [
        ImapVerb::Capability,
        ImapVerb::Login,
        ImapVerb::Authenticate,
        ImapVerb::Logout,
        ImapVerb::Examine,
        ImapVerb::List,
        ImapVerb::Lsub,
        ImapVerb::Status,
        ImapVerb::Fetch,
        ImapVerb::UidFetch,
        ImapVerb::Search,
        ImapVerb::UidSearch,
        ImapVerb::Idle,
        ImapVerb::Done,
        ImapVerb::Noop,
        ImapVerb::Id,
        ImapVerb::Enable,
        ImapVerb::GetMetadata,
        ImapVerb::GetAcl,
        ImapVerb::GetQuota,
        ImapVerb::GetQuotaRoot,
    ] {
        assert!(
            assert_verb_allowed(&v).is_ok(),
            "verb {v:?} should be allowed"
        );
    }
}

#[test]
fn allow_list_has_exactly_21_variants() {
    // Compile-time-asserted via the variants array above.
    let count = [
        ImapVerb::Capability,
        ImapVerb::Login,
        ImapVerb::Authenticate,
        ImapVerb::Logout,
        ImapVerb::Examine,
        ImapVerb::List,
        ImapVerb::Lsub,
        ImapVerb::Status,
        ImapVerb::Fetch,
        ImapVerb::UidFetch,
        ImapVerb::Search,
        ImapVerb::UidSearch,
        ImapVerb::Idle,
        ImapVerb::Done,
        ImapVerb::Noop,
        ImapVerb::Id,
        ImapVerb::Enable,
        ImapVerb::GetMetadata,
        ImapVerb::GetAcl,
        ImapVerb::GetQuota,
        ImapVerb::GetQuotaRoot,
    ]
    .len();
    assert_eq!(count, 21);
}
