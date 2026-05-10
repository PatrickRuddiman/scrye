# Triage: OAuth / token-based IMAP auth

**Status:** Deferred to v0.3.5. App passwords only in v0.3.x.

**Why deferred:** OAuth flow is provider-specific (Google, Microsoft, Yahoo each different); refresh-token storage is its own credential class. Out of v0.3.1's scope which is the architectural pivot + finishing v0.3.0's deferred items.

**Triggers:** Operators on Google Workspace / Microsoft 365 where 2FA is mandatory and app passwords are disabled. Already common.

**Sketch:** new `[[accounts]] auth = "oauth"` discriminator; per-provider refresh-token + access-token cache; renew on 401. Wire into `scryd_imap::connect::login` as a third branch alongside `login_tls` / `login_plain`.
