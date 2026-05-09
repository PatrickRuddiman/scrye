Parent plan: GreenMail integration testing for scryd
Depends on: 08

# Task 09 — e2e-imap-to-search

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Stand up the true end-to-end test: a privileged-Docker harness that installs scryd, points it at a GreenMail container, injects N messages via SMTP, lets the daemon index, and asserts `scryd search` returns hits. Wire into `release.yml` as a CI step alongside the existing install smoke.

## Tasks
- [x] Create `tests/e2e_imap_to_search.sh` modeled on `tests/install_sh.sh`:
  - Self-relaunch via `docker run` if not inside a container — same self-detection pattern (`/.dockerenv` + `SCRYD_SMOKE_INNER`).
  - Inside the container: `apt-get install -y systemd sudo passwd coreutils curl bsd-mailx`.
  - Use `--network host` (or a docker-compose pair) so the inner container can reach the GreenMail container at `127.0.0.1:3025` / `:3143`.
  - Build the fixture bundle (binaries + scryd.service.in + scryd.tmpfiles.in + LICENSE).
  - `useradd -m alice`.
  - `./install.sh --user alice --skip-weights --skip-systemctl`.
  - Write `/etc/scryd/config.toml` (as root, owned scryd:scryd 0600) with one account: `id = "primary"`, `host = "127.0.0.1"`, `port = 3143`, `user = "test"`, `password = "test"`, `tls = false`, `folders = ["INBOX"]`.
  - Inject 50 messages into GreenMail via `curl -X POST` to GreenMail's SMTP (or use `bsd-mailx`/`swaks`). Pick the lightest dependency.
  - Start `sudo -u scryd /usr/local/bin/scryd serve &` (NOT systemd — direct).
  - Poll `sqlite3 /var/lib/scryd/meta.sqlite 'SELECT count(*) FROM messages'` until 50 or 30s budget exhausted.
  - As alice (no sudo): `scryd search "test"` → assert ≥ 1 hit.
  - Print `OK: e2e indexing + search round-trip passed (50 messages, ≥ 1 hits)` and exit 0; on any assertion failure, `FAIL: <what>` and exit non-zero.
- [x] Make the script executable.
- [x] Update `.github/workflows/release.yml` build job to add a `services:` block:
  ```yaml
  services:
    greenmail:
      image: greenmail/standalone:latest
      env:
        GREENMAIL_OPTS: "-Dgreenmail.users=test:test@localhost -Dgreenmail.hostname=0.0.0.0 -Dgreenmail.auth.disabled=true"
      ports:
        - 3025:3025
        - 3143:3143
        - 8080:8080
  ```
  Add a step `bash tests/e2e_imap_to_search.sh` after the install smoke, gated `if: matrix.cross == false`.
- [x] Document local-dev usage in `ops/README.install.md` testing section: instructions to run `docker run greenmail/standalone:latest` then `bash tests/e2e_imap_to_search.sh`.

## Acceptance criteria
- [x] `test -x tests/e2e_imap_to_search.sh`.
- [x] `bash -n tests/e2e_imap_to_search.sh` exits 0.
- [x] Locally (with Docker + GreenMail container running): `bash tests/e2e_imap_to_search.sh` exits 0 and prints the success line.
- [x] `grep -F 'greenmail/standalone' .github/workflows/release.yml` matches the services block.
- [x] `grep -F 'bash tests/e2e_imap_to_search.sh' .github/workflows/release.yml` matches the new step.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
