# Triage: SysV-init / OpenRC / runit support

**Status:** Out of v0.3.1. systemd-only.

**Why deferred:** Vast majority of modern Linux deploys use systemd. Each alternative supervisor needs its own unit-file template + `install.sh` branch + e2e harness adjustment.

**Triggers:** A consumer or deploy on Alpine (OpenRC), Void (runit), or a vintage Debian (SysV) where systemd isn't available.

**Sketch:** rename `ops/scryd.service.in` → `ops/systemd/scryd.service.in`; add `ops/openrc/scryd.initd` + `ops/runit/run`. install.sh detects the active supervisor and copies the right template.
