Parent plan: scryd v0.3.1 — service pivot
Depends on: 02

# Task 03 — drop-user-substitution

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Strip `__USER__` and `__UID__` substitution from the install pipeline. The systemd unit no longer sets `SCRYD_ALLOWED_UID`. The runtime dir becomes plain `scryd:scryd 0755`. The install is no longer per-operator; `--user` and `--remove-v01-data` flags go away.

## Tasks
- [x] In `ops/scryd.service.in`, delete the line `Environment=SCRYD_ALLOWED_UID=__UID__`. Other `Environment=` lines stay.
- [x] In `ops/scryd.tmpfiles.in`, change `d /run/scryd 0750 scryd __USER__ -` to `d /run/scryd 0755 scryd scryd -`.
- [x] In `ops/install.sh`, delete:
  - The `ALLOWED_USER` resolution from `$SUDO_USER` and `--user <name>` flag (lines around 28-65).
  - The `ALLOWED_UID` resolution via `getent passwd`.
  - The `--remove-v01-data` flag, the `V01_FINDINGS` / `V01_FILES` arrays, the `detect_v01_users` function, the v0.1.0-detection-and-bail block, and the v0.1.0-removal block (everything between the v0.1.0 fixture detection and the system-user creation).
  - The `sed -e "s/__UID__/...` and `sed -e "s/__USER__/...` substitution lines for both templates; replace with plain `install -m 0644 -o root -g root "$SCRIPT_DIR/scryd.service.in" /etc/systemd/system/scryd.service` and the analogous tmpfiles copy.
  - The `install -d -m 0750 -o scryd -g "$ALLOWED_USER" /run/scryd` line in the `--skip-systemctl` branch; replace with `install -d -m 0755 -o scryd -g scryd /run/scryd`.
  - The next-steps echo block's reference to "operator '$ALLOWED_USER' (uid $ALLOWED_UID)"; print a generic "scryd v0.3.1 installed" line instead.
- [x] In `ops/install.sh`, change `install -m 0600 -o scryd -g scryd /dev/null /etc/scryd/config.toml` to `install -m 0640 -o scryd -g scryd /dev/null /etc/scryd/config.toml` so an admin running `cat /etc/scryd/config.toml` (without sudo) gets `EACCES` only when not in the scryd group; root + scryd both have access.
- [x] In `ops/install.sh`, drop the `bsdmainutils`/`useradd` flags that were per-operator; keep `useradd --system --shell /usr/sbin/nologin --home /var/lib/scryd scryd`.
- [x] In `ops/install.sh`, the install becomes flag-light: only `--skip-weights` and `--skip-systemctl` remain (test escapes).
- [x] Verify with the existing smoke harness: `bash tests/install_sh.sh` continues to pass once tasks 04 (CLI elevation) + 13 (test refresh) land. For now, don't update the smoke; it'll fail on Scenario 2 / 5 and that's expected — task 13 reframes them.

## Acceptance criteria
- [x] `bash -n ops/install.sh` exits 0.
- [x] `! grep -F '__UID__' ops/scryd.service.in`.
- [x] `! grep -F '__USER__' ops/scryd.tmpfiles.in`.
- [x] `grep -F 'd /run/scryd 0755 scryd scryd -' ops/scryd.tmpfiles.in` matches.
- [x] `! grep -F 'SCRYD_ALLOWED_UID' ops/scryd.service.in`.
- [x] `! grep -F '--remove-v01-data' ops/install.sh`.
- [x] `! grep -F '--user' ops/install.sh`.
- [x] `grep -F 'install -m 0640 -o scryd -g scryd /dev/null /etc/scryd/config.toml' ops/install.sh` matches.
- [x] `bash -c 'sed -e "s/__UID__/1000/g" -e "s/__USER__/scryd/g" ops/scryd.service.in | systemd-analyze verify /dev/stdin' returns 0` (verify the rendered template still parses; substitution is a no-op now).

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
