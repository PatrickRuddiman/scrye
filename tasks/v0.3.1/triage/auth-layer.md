# Triage: auth at scryd's API layer

**Status:** Deliberately deferred in v0.3.1. The consumer's higher-layer API is the auth boundary; scryd ships open. See `docs/security.md`.

**Why deferred:** scryd is positioned as a service cog. Re-implementing OAuth/JWT/RBAC inside scryd would force every consumer onto scryd's choices and duplicates work the consumer's API surface already does.

**Triggers for picking it up:**
- Multiple consumers want a shared scryd instance and a per-consumer rate-limiter / audit trail.
- A deploy puts scryd on a network where the only sensible boundary is at the daemon itself.
- Spec §3 In needs `[server] auth = "bearer"` or similar.

**Sketch:** add `[server] auth = "bearer"` config; daemon checks `Authorization: Bearer <token>` against a token list at /etc/scryd/tokens. Each token carries an optional `account_ids` allowlist; the daemon intersects with the request's filter.
