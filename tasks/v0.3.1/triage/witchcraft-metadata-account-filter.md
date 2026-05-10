# Triage: push account_ids filter into witchcraft's metadata SQL

**Status:** v0.3.5 perf optimisation. Today the api post-filters by account_id after witchcraft returns hits.

**Why deferred:** Post-filter works correctly. The performance penalty only matters when an account holds millions of messages and the filter excludes most of them.

**Triggers:** Consumer reports search latency at the spec §4 p99 boundary on a fixture with many accounts.

**Sketch:** `WitchcraftIndexer::submit` already stashes the message_id in the metadata blob; extend it to stash account_id too; pass `account_ids` into `witchcraft::search()`'s `sql_filter` parameter as `metadata->>'account_id' IN (...)`.
