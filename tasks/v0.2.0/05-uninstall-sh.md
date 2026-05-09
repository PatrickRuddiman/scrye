Parent slice: [scryd v0.2.0 — build-and-packaging](../../slices/0.2.0/build-and-packaging.md)
Depends on: 04

# Task 05 — uninstall-sh

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Ship `ops/uninstall.sh` that removes every artifact `ops/install.sh` produces — the system service, unit file, tmpfiles drop-in, FHS directories, binaries, and the `scryd` Linux account — idempotently, requiring root.

## Tasks
- [ ] Create `ops/uninstall.sh` with `#!/usr/bin/env bash` and `set -euo pipefail`.
- [ ] Refuse to run if `[[ $EUID -ne 0 ]]`. Exit 1 with stderr message.
- [ ] Stop and disable the unit: `systemctl disable --now scryd 2>/dev/null || true`. Best-effort — if the unit doesn't exist (partial install), the script continues.
- [ ] Remove the unit file and tmpfiles drop-in: `rm -f /etc/systemd/system/scryd.service /etc/tmpfiles.d/scryd.conf`.
- [ ] Remove the binaries: `rm -f /usr/local/bin/scryd /usr/local/bin/scryd-fetch-weights`.
- [ ] Remove the FHS directories: `rm -rf /etc/scryd /var/lib/scryd /run/scryd`.
- [ ] Remove the system user: `userdel scryd 2>/dev/null || true`. Idempotent (no-op if user doesn't exist).
- [ ] Reload systemd so it forgets the now-removed unit: `systemctl daemon-reload`.
- [ ] Print a one-line confirmation message naming what was removed (or "nothing was installed" if all the rm/userdel calls were no-ops). Use a counter incremented per successful removal step.
- [ ] Make executable at commit time.

## Acceptance criteria
- [ ] `test -x ops/uninstall.sh`.
- [ ] `bash -n ops/uninstall.sh` exits 0.
- [ ] `grep -E '\[\[ \$EUID -ne 0 \]\]' ops/uninstall.sh` matches the root requirement.
- [ ] `grep -F 'systemctl disable --now scryd' ops/uninstall.sh` matches.
- [ ] `grep -F 'rm -rf /etc/scryd /var/lib/scryd /run/scryd' ops/uninstall.sh` matches.
- [ ] `grep -F 'userdel scryd' ops/uninstall.sh` matches.
- [ ] `grep -F 'systemctl daemon-reload' ops/uninstall.sh` matches.
- [ ] `bash tests/install_sh.sh` (smoke harness from task 07) covers the install→uninstall round-trip and exits 0 once task 07 has landed; verifies idempotency by running uninstall twice in a row inside the container.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
