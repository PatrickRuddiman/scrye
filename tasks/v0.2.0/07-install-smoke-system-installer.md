Parent slice: [scryd v0.2.0 — build-and-packaging](../../slices/0.2.0/build-and-packaging.md), [scryd v0.2.0 — migration](../../slices/0.2.0/migration.md)
Depends on: 04, 05

# Task 07 — install-smoke-system-installer

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Rewrite `tests/install_sh.sh` so it exercises the v0.2.0 system installer (which needs root + systemd-tmpfiles + useradd) inside a privileged `debian:bookworm` Docker container, covering five scenarios: fresh install, isolation property (different user can't read config), v0.2.x re-install idempotency, uninstall round-trip, v0.1.0 → v0.2.0 transition with and without `--remove-v01-data`.

## Tasks
- [x] Rewrite `tests/install_sh.sh` from scratch with `#!/usr/bin/env bash` and `set -euo pipefail`.
- [x] At the top of the script, detect whether already running inside the container: if `/.dockerenv` exists OR the env var `SCRYD_SMOKE_INNER` is set, skip the docker re-launch; otherwise run `exec docker run --privileged --rm -e SCRYD_SMOKE_INNER=1 -v "$(git rev-parse --show-toplevel):/workspace" -w /workspace debian:bookworm bash tests/install_sh.sh`.
- [x] Inside the container, install prerequisites: `apt-get update && apt-get install -y --no-install-recommends systemd sudo passwd coreutils`. Do NOT start systemd inside the container — `systemd-tmpfiles --create` works without an active systemd PID 1; `systemctl enable --now` does not, so the harness uses `--skip-systemctl` for the install.
- [x] Build the fixture bundle in a tempdir: copy `ops/install.sh`, `ops/uninstall.sh`, `ops/scryd.service.in`, `ops/scryd.tmpfiles.in`, `LICENSE` into the bundle dir. Stub `scryd` and `scryd-fetch-weights` as 3-line bash scripts that print `fake-scryd "$@"` and exit 0.
- [x] Add two test users: `useradd -m alice`, `useradd -m mallory`.
- [x] **Scenario 1 — Fresh install.** Run `(cd "$BUNDLE" && ./install.sh --user alice --skip-weights --skip-systemctl)`. Assert exit 0. Assert: `/etc/scryd/config.toml` exists owned `scryd:scryd` mode 0600. `/var/lib/scryd/` exists owned `scryd:scryd` mode 0700. `/etc/systemd/system/scryd.service` exists. `/etc/tmpfiles.d/scryd.conf` exists. `/run/scryd/` exists owned `scryd:alice` mode 0750. The rendered unit contains `User=scryd` and `SCRYD_ALLOWED_UID=$(id -u alice)`. Use `stat -c '%U:%G %a'` for ownership/mode assertions.
- [x] **Scenario 2 — Isolation property.** As mallory (`su mallory -c 'cat /etc/scryd/config.toml'`), assert exit non-zero and stderr contains `Permission denied`. As alice (`su alice -c 'cat /etc/scryd/config.toml'`), assert the same — alice doesn't own the file either; only the daemon UID does.
- [x] **Scenario 3 — v0.2.x re-install idempotency.** Append a synthetic `[[accounts]]` entry to `/etc/scryd/config.toml` (write as root, chown back to `scryd:scryd`). Compute its sha256. Re-run `./install.sh --user alice --skip-weights --skip-systemctl`. Assert exit 0. Assert the config file's sha256 is unchanged.
- [x] **Scenario 4 — Uninstall round-trip.** Run `./uninstall.sh`. Assert exit 0. Assert: `/etc/systemd/system/scryd.service` is gone, `/etc/scryd/` is gone, `/var/lib/scryd/` is gone, `/run/scryd/` is gone, `/usr/local/bin/scryd` is gone, `getent passwd scryd` returns empty (user deleted). Run `./uninstall.sh` a second time, assert exit 0 (idempotent).
- [x] **Scenario 5 — v0.1.0 → v0.2.0 transition.** Re-create the v0.1.0 layout for alice: `mkdir -p /home/alice/.local/bin /home/alice/.config/systemd/user /home/alice/.config/scryd`. Touch the v0.1.0 sentinel files: `/home/alice/.local/bin/scryd`, `/home/alice/.local/bin/scryd-fetch-weights`, `/home/alice/.config/systemd/user/scryd.service`, `/home/alice/.config/scryd/config.toml`. `chown -R alice:alice /home/alice/.local /home/alice/.config`. Run `./install.sh --user alice --skip-weights --skip-systemctl` (without `--remove-v01-data`). Assert exit 2. Assert stderr contains `/home/alice/.local/bin/scryd`. Assert no v0.2.0 artifacts present (`! test -e /etc/scryd`, etc.). Re-run with `--remove-v01-data`. Assert exit 0. Assert all four v0.1.0 sentinel paths are gone. Assert v0.2.0 layout from Scenario 1 is present.
- [x] On any scenario failure, print `FAIL: <scenario name>: <what was expected vs what was found>` and exit non-zero. On all-pass, print `OK: install.sh smoke test passed (5 scenarios)` and exit 0.
- [x] Wire the test into `.github/workflows/release.yml`: add a step in the build job (Linux-only by definition now) running `bash tests/install_sh.sh` after `cargo build --release` and before `package tarball`. The runner already has Docker.

## Acceptance criteria
- [x] `test -x tests/install_sh.sh`.
- [x] `bash -n tests/install_sh.sh` exits 0.
- [x] `grep -F 'docker run --privileged' tests/install_sh.sh` matches the docker re-launch.
- [x] `grep -F 'SCRYD_SMOKE_INNER' tests/install_sh.sh` matches the inner-mode signal.
- [x] `grep -F '--remove-v01-data' tests/install_sh.sh` matches Scenario 5.
- [x] `grep -E 'OK: install.sh smoke test passed' tests/install_sh.sh` matches the success line.
- [x] `grep -F 'bash tests/install_sh.sh' .github/workflows/release.yml` matches the CI wiring.
- [x] On a host with Docker available: `bash tests/install_sh.sh` exits 0 and prints the success line. (On hosts without Docker, this AC item is skipped; the CI wiring is the gate.)

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
