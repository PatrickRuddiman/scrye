Parent slice: [mcp](../slices/mcp.md)
Depends on: 28

# Task 29 — scryd-mcp read tools (scoped)

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Six read tools — `search`, `get_message`, `get_raw_message`, `get_thread`, `list_accounts`, `status` — return structured results restricted to `USER_EMAIL`'s accounts, porting the existing handler logic and refusing foreign-account data.

## Tasks
- [x] Create `crates/scryd-mcp/src/dto.rs` porting the response shapes from `crates/scryd-api/src/dto.rs` (`SearchHitDto`, `SearchResponseDto`, `AddressDto`, `AttachmentDto`, `MessageDto`, `ThreadResponseDto`, `AccountDto`, `AccountsResponseDto`, and consts `LIMIT_DEFAULT = 20`, `LIMIT_MAX = 200`), plus per-tool input arg structs deriving `serde::Deserialize` + `schemars::JsonSchema` (`SearchArgs` with `q`/`from`/`since`/`until`/`folder`/`account_ids`/`limit`/`mode`; `IdArg { id }`).
- [x] Create `crates/scryd-mcp/src/tools/read.rs` with a `search` tool porting `crates/scryd-api/src/handlers.rs:22` (mode parse, `since`/`until` ISO parse, `limit` clamp to `1..=LIMIT_MAX`, `k = max(limit*K_MULTIPLIER, K_MIN)`, snippet via a ported `derive_snippet` from `handlers.rs:168`, row filtering via a ported `filter_match` from `handlers.rs:135`) that forces the search `account_ids` to `scope.allowed_account_ids(&storage)` intersected with any caller `account_ids` via `scope::narrow` (narrow-only, never widen).
- [x] Add `get_message` (port `crates/scryd-api/src/handlers.rs:183`) and `get_raw_message` (port `handlers.rs:196`, read `row.raw_path`), each returning a `not_found` `McpError` when `row.account_id` is not in the allowed set or the row is tombstoned/missing.
- [x] Add `get_thread` (port `handlers.rs:226`) dropping messages whose `account_id` is not allowed and returning `not_found` when none remain; `list_accounts` (port `handlers.rs:246`) limited to allowed accounts; `status` (port `handlers.rs:269`) with the per-account block limited to allowed accounts and the issue-#20 `drainer`/`daemon` blocks plus `ok = !daemon_health.in_crash_loop` passed through.
- [x] Map outcomes to `McpError`: bad `mode`/date → `invalid_params`; missing/out-of-scope id → `not_found`; backend error → `internal_error`.
- [x] Create `crates/scryd-mcp/tests/read_tools.rs` seeding a temp `StorageHandle` with an owned account + a foreign account and messages in each, then asserting: `search` returns only owned hits; `get_message`/`get_raw_message`/`get_thread` on a foreign-owned id return a `not_found` error; `list_accounts` and `status` expose only the owned account.

## Acceptance criteria
- [x] `cargo test -p scryd-mcp --test read_tools` passes (integration: real temp storage; scope enforcement on every read tool).
- [x] `git grep -nE "fn (search|get_message|get_raw_message|get_thread|list_accounts|status)\b" crates/scryd-mcp/src` matches 6.
- [x] `cargo build -p scryd-mcp` exits 0.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
