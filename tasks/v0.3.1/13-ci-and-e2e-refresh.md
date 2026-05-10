Parent plan: scryd v0.3.1 — service pivot
Depends on: 01, 02, 03, 04, 05, 06, 07, 08, 09, 10, 11, 12

# Task 13 — ci-and-e2e-refresh

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Refresh the privileged-docker install smoke and the e2e indexing+search smoke for the new defaults: open socket, no `--user`/`--remove-v01-data`, witchcraft persistence, account_ids filter. Cache the weights file in CI so the e2e doesn't pull 1 GB on every push.

## Tasks
- [ ] Update `tests/install_sh.sh`:
  - Drop Scenario 5 entirely (v0.1.0 → v0.2.0 migration; no longer relevant).
  - Reframe Scenario 2 (Isolation property) as "search socket is reachable by any local user; config file remains scryd-owned": as alice → `nc -zU /run/scryd/scryd.sock` (or equivalent test-only `scryd search` against the socket if the daemon were running) succeeds; as alice → `cat /etc/scryd/config.toml` exits non-zero with EACCES (file mode 0640 scryd:scryd, alice not in scryd group).
  - Drop the `--user alice` / `--remove-v01-data` invocations; install.sh runs flag-light.
  - Scenarios 1, 3, 4 stay; update assertion paths if any of the chown/mode assertions need to change for 0666 socket / 0755 runtime dir / 0640 config.
- [ ] Update `tests/e2e_imap_to_search.sh`:
  - After the existing "50 messages indexed → search returns hits" leg, add: `kill -TERM $SCRYD_PID; wait $SCRYD_PID; <restart with same XDG_DATA_HOME>; scryd search "e2e" --json | jq '.hits | length' >= 1` — proves witchcraft persistence.
  - Add a second-account leg: write a second `[[accounts]]` with `id = "secondary"`; restart daemon; inject 10 messages from a different sender; query `scryd search e2e --accounts secondary --json` and assert all returned hits have `account_id == "secondary"`.
  - Update the success line to `OK: e2e indexing + search round-trip + persistence + account_ids filter passed`.
- [ ] Update `.github/workflows/ci.yml`:
  - Add an `actions/cache@v4` step keyed on `weights-${{ hashFiles('crates/scryd-fetch-weights/src/main.rs') }}` (the constant pin invalidates the cache when the SHA-256 changes), restoring `/var/lib/scryd/assets/xtr-weights.gguf`. The cache step runs before the e2e step.
  - On a cold cache, the e2e leg lets the daemon auto-fetch (task 08); subsequent runs hit the cache.
- [ ] Update `.github/workflows/release.yml` to mirror the same cache step before the e2e leg.
- [ ] Refresh `crates/scryd-api/tests/peercred.rs` and `peercred_accept_path.rs`: most tests already pass `enabled = true` per task 01; ensure the test names + assertions reflect that peercred is now opt-in (e.g., add a class-comment naming the v0.3.1 default).

## Acceptance criteria
- [ ] `bash -n tests/install_sh.sh` exits 0.
- [ ] `bash -n tests/e2e_imap_to_search.sh` exits 0.
- [ ] Locally with GreenMail running: `bash tests/e2e_imap_to_search.sh` exits 0 and prints the new success line.
- [ ] Locally: `bash tests/install_sh.sh` exits 0 (4 scenarios pass; Scenario 5 deleted).
- [ ] `grep -F 'actions/cache' .github/workflows/ci.yml` matches the weights cache step.
- [ ] `grep -F 'witchcraft.sqlite' tests/e2e_imap_to_search.sh` matches OR the persistence leg's restart-and-search assertion is otherwise visible.
- [ ] `! grep -F '--remove-v01-data' tests/install_sh.sh`.
- [ ] `grep -F '--accounts secondary' tests/e2e_imap_to_search.sh` matches the multi-account filter assertion.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
