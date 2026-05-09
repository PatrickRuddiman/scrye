Parent slice: [scryd v0.2.0 — build-and-packaging](../../slices/0.2.0/build-and-packaging.md), [scryd v0.2.0 — migration](../../slices/0.2.0/migration.md)
Depends on: 03

# Task 04 — install-sh-system-rewrite

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Rewrite `ops/install.sh` as a system installer that requires root, creates the dedicated `scryd` Linux account, lays out the FHS directory tree, renders the systemd unit + tmpfiles drop-in from `.in` templates, downloads weights as the `scryd` user, enables the system service, and idempotently handles re-runs (v0.2.x → v0.2.(x+1)) plus v0.1.0 → v0.2.0 transitions via `--remove-v01-data`.

## Tasks
- [x] Rewrite `ops/install.sh` from scratch. The script starts with `#!/usr/bin/env bash` and `set -euo pipefail`.
- [x] Refuse to run if `[[ $EUID -ne 0 ]]`. Exit code 1 with stderr message naming the requirement.
- [x] Parse flags via `while [[ $# -gt 0 ]]; case` block: `--user <name>` (defaults to `${SUDO_USER:-}`; resolves to UID via `getent passwd <name>` and validates non-empty), `--remove-v01-data` (boolean, default unset), `--skip-weights` (boolean, test-only), `--skip-systemctl` (boolean, test-only).
- [x] Define `detect_v01_users()` function. Iterates `getent passwd | awk -F: '$3 >= 1000 && $3 < 65534 {print $1":"$6}'`. For each `<user>:<home>` pair, tests existence of: `<home>/.local/bin/scryd`, `<home>/.local/bin/scryd-fetch-weights`, `<home>/.config/systemd/user/scryd.service`, `<home>/.config/scryd/config.toml`. Appends found entries to a global bash array `V01_FINDINGS`.
- [x] If `V01_FINDINGS` is non-empty and `--remove-v01-data` was not passed: print every finding, print the spec's explanation block ("scryd v0.2.0 ships a different on-disk layout than v0.1.0..."), exit 2.
- [x] If `V01_FINDINGS` is non-empty and `--remove-v01-data` was passed: per detected user, run `sudo -u <user> systemctl --user stop scryd 2>/dev/null || true`, `sudo -u <user> systemctl --user disable scryd 2>/dev/null || true`, then `rm -f <home>/.local/bin/scryd <home>/.local/bin/scryd-fetch-weights <home>/.config/systemd/user/scryd.service`, `rm -rf <home>/.config/scryd <home>/.local/share/scryd`. Log one line per user.
- [x] Resolve `SCRIPT_DIR` (the install.sh's own directory) and verify the bundled artifacts: `scryd`, `scryd-fetch-weights`, `scryd.service.in`, `scryd.tmpfiles.in`, `LICENSE`. Exit with stderr error if any missing.
- [x] Create system user idempotently: `if ! getent passwd scryd >/dev/null; then useradd --system --shell /usr/sbin/nologin --home /var/lib/scryd scryd; fi`.
- [x] Create directories with `install -d -m 0700 -o scryd -g scryd /etc/scryd /var/lib/scryd` and `install -d -m 0755 -o scryd -g scryd /var/lib/scryd/assets`.
- [x] Seed `/etc/scryd/config.toml` only if missing: `if [[ ! -e /etc/scryd/config.toml ]]; then install -m 0600 -o scryd -g scryd /dev/null /etc/scryd/config.toml; fi`. This preserves operator config across re-installs.
- [x] Install binaries: `install -m 0755 "$SCRIPT_DIR/scryd" /usr/local/bin/scryd`, same for `scryd-fetch-weights`.
- [x] Render templates with `sed -e "s/__UID__/$ALLOWED_UID/g" -e "s/__USER__/$ALLOWED_USER/g" "$SCRIPT_DIR/scryd.service.in" > /etc/systemd/system/scryd.service` (mode 0644 root:root). Same pattern for `scryd.tmpfiles.in` → `/etc/tmpfiles.d/scryd.conf`.
- [x] Provision `/run/scryd/`: run `systemd-tmpfiles --create /etc/tmpfiles.d/scryd.conf`. If `systemd-tmpfiles` is missing (non-systemd host), exit with a clear error.
- [x] If `--skip-weights` not set: `sudo -u scryd /usr/local/bin/scryd-fetch-weights --target /var/lib/scryd/assets/`. After success, `chmod 0644 /var/lib/scryd/assets/xtr-weights.gguf` (the helper writes 0600; weights are public, daemon needs world-readable for `mmap`).
- [x] If `--skip-systemctl` not set: `systemctl daemon-reload; systemctl enable --now scryd`.
- [x] Print the spec's next-steps block (one heredoc): naming `sudo scryd add-account`, `journalctl -u scryd -f`, `sudo systemctl restart scryd` after config changes.
- [x] Make the script executable: `chmod +x ops/install.sh` at commit time.

## Acceptance criteria
- [x] `test -x ops/install.sh`.
- [x] `bash -n ops/install.sh` exits 0.
- [x] `grep -E '\[\[ \$EUID -ne 0 \]\]' ops/install.sh` matches the root requirement.
- [x] `grep -F 'useradd --system --shell /usr/sbin/nologin --home /var/lib/scryd scryd' ops/install.sh` matches.
- [x] `grep -F 'install -d -m 0700 -o scryd -g scryd /etc/scryd' ops/install.sh` matches.
- [x] `grep -F 'systemd-tmpfiles --create /etc/tmpfiles.d/scryd.conf' ops/install.sh` matches.
- [x] `grep -F 'sudo -u scryd' ops/install.sh` matches the weights-fetch line.
- [x] `grep -F '--remove-v01-data' ops/install.sh` matches.
- [x] `grep -F 'getent passwd' ops/install.sh` matches the v0.1.0 detection walk.
- [x] `grep -E 'sed -e "s/__UID__/' ops/install.sh` matches the template rendering.
- [x] `grep -F 'systemctl enable --now scryd' ops/install.sh` matches.
- [x] `bash tests/install_sh.sh` (the smoke test from task 07) exits 0 once task 07 has landed; before that, this AC item is skipped.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
