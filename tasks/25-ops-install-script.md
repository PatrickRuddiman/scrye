Parent slice: [build-and-packaging](../slices/build-and-packaging.md)
Depends on: 24

# Task 25 — ops-install-script

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Ship the per-user `install.sh` that places the binary, places the systemd unit, fetches weights, and prints the post-install hint block, plus the operator-facing `README.install.md`.

## Tasks
- [ ] Create `ops/install.sh` with `#!/usr/bin/env bash` and `set -euo pipefail`. Must work under bash 4+ (Ubuntu 22.04 default).
- [ ] First action: refuse to run as root — `if [[ $EUID -eq 0 ]]; then echo "run as your own user, not root" >&2; exit 1; fi`.
- [ ] Resolve `$HOME`, `$XDG_CONFIG_HOME` (default `$HOME/.config`), `$XDG_DATA_HOME` (default `$HOME/.local/share`).
- [ ] Resolve the script's own directory and find the bundled binaries: `scryd`, `scryd-fetch-weights`. Both must exist relative to `install.sh` (the tarball ships them next to the script).
- [ ] Create `~/.local/bin/`, `~/.config/systemd/user/`, `$XDG_CONFIG_HOME/scryd/` (mode `0700`), `$XDG_DATA_HOME/scryd/` (mode `0700`), `$XDG_DATA_HOME/scryd/assets/` (mode `0700`) if absent.
- [ ] Copy `scryd` and `scryd-fetch-weights` to `~/.local/bin/` mode `0755`.
- [ ] Copy `scryd.service` to `~/.config/systemd/user/scryd.service`.
- [ ] If `~/.local/bin` is not in `$PATH`, print a yellow warning suggesting the operator add it to their shell rc.
- [ ] Run `~/.local/bin/scryd-fetch-weights --target "$XDG_DATA_HOME/scryd/assets/"`. On failure, abort with a clear error.
- [ ] Run `systemctl --user daemon-reload`. (If `systemctl` is missing — non-systemd host — print a clear message and exit 1.)
- [ ] Print the post-install hint block exactly per build-and-packaging slice §3 Decision 7:
  - `next steps:`
  - `  scryd add-account`
  - `  systemctl --user enable --now scryd`
  - `optional (survive logout / start at boot):`
  - `  loginctl enable-linger $USER`
- [ ] Make `install.sh` executable in version control (`chmod +x` at commit time; the `[ -x ops/install.sh ]` AC verifies this).
- [ ] Create `ops/README.install.md` documenting:
  - the install steps `install.sh` automates,
  - the manual install path (copy the binary to `~/.local/bin`, copy the unit, download weights via the documented URL + SHA-256),
  - the uninstall path: `systemctl --user disable --now scryd; rm ~/.local/bin/scryd ~/.config/systemd/user/scryd.service; rm -rf ~/.config/scryd ~/.local/share/scryd`,
  - the operator-facing journal recipes from observability slice §4 (`journalctl --user -u scryd …`).
- [ ] Write a shell-based smoke test in `tests/install_sh.bats` (or a plain `tests/install_sh.sh`) using a temp `HOME`: invoke `install.sh` against a dummy bundle (where `scryd` and `scryd-fetch-weights` are tiny shell scripts that print "ok"); assert `~/.local/bin/scryd` exists, `~/.config/systemd/user/scryd.service` exists, `$XDG_DATA_HOME/scryd/` exists with mode `0700`. Provide the test fixture under `ops/test_fixtures/`.

## Acceptance criteria
- [ ] `test -x ops/install.sh`.
- [ ] `bash -n ops/install.sh` (syntax check) exits 0.
- [ ] `bash tests/install_sh.sh` (the smoke harness) exits 0 and creates the expected per-user layout under a temp `HOME`.
- [ ] `grep -E '\[\[\s*\$EUID\s*-eq\s*0\s*\]\]' ops/install.sh` matches the no-root guard.
- [ ] `grep -E 'systemctl --user daemon-reload' ops/install.sh` matches.
- [ ] `grep -F 'loginctl enable-linger' ops/install.sh` matches.
- [ ] `test -f ops/README.install.md && grep -F 'journalctl --user -u scryd' ops/README.install.md` matches.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
