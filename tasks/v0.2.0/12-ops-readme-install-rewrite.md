Parent slice: [scryd v0.2.0 — build-and-packaging](../../slices/0.2.0/build-and-packaging.md), [scryd v0.2.0 — migration](../../slices/0.2.0/migration.md)
Depends on: 04, 05, 09

# Task 12 — ops-readme-install-rewrite

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Rewrite `ops/README.install.md` to walk the v0.2.0 system-install flow: one-command sudo install, isolation properties table, daily-use commands with sudo notes, daemon reload after config edits, uninstall, v0.1.0 → v0.2.0 transition, v0.2.x upgrade, and journalctl recipes for the system unit.

## Tasks
- [ ] Rewrite `ops/README.install.md` with the following sections in order:
  1. **Quick install.** One block: `tar -xzf scryd-vX.Y.Z-x86_64-linux.tar.gz`, `cd scryd-vX.Y.Z-x86_64-linux`, `sudo ./install.sh`. Mention `--user <name>` if the operator wants to install for a user other than `$SUDO_USER`.
  2. **What the install creates.** Bullet list mirroring build-and-packaging slice §4 (FHS layout): `/usr/local/bin/scryd`, `/etc/scryd/config.toml` (`scryd:scryd` 0600), `/var/lib/scryd/`, `/run/scryd/`, `/etc/systemd/system/scryd.service`, `/etc/tmpfiles.d/scryd.conf`. One line per artifact.
  3. **Isolation properties.** The alice / mallory table from build-and-packaging slice §1 of the v0.2.0 spec. Operations: `open(/etc/scryd/config.toml)`, `connect(/run/scryd/scryd.sock)`, `ptrace daemon`, `read /var/lib/scryd/...`. Outcomes: ✅ / ❌ + one-line reason.
  4. **First account configuration.** Block: `sudo scryd add-account` (interactive). After success: `sudo systemctl restart scryd`. If the daemon was not running yet: the CLI prints `sudo systemctl start scryd` instead.
  5. **Daily use.** `scryd search "..."` (no sudo). `scryd reindex` (no sudo). For mutations: `sudo scryd add-account`, `sudo scryd rotate-password <id>`, `sudo scryd remove-account <id>`, each followed by `sudo systemctl restart scryd`.
  6. **Logs.** `journalctl -u scryd -f` (live tail). `journalctl -u scryd --output cat -n 200`. `journalctl -u scryd --output json | jq 'select(.MESSAGE | contains(...))'` for filtering by category. (No `--user` flag — it's a system unit now.)
  7. **Uninstall.** `sudo ./uninstall.sh` from inside the extracted tarball. List exactly what gets removed.
  8. **v0.1.0 → v0.2.0 migration.** Explain detection, the `--remove-v01-data` flag, that v0.1.0 data is removed wholesale, that the operator re-runs `sudo scryd add-account` and the daemon resyncs from IMAP. Cross-reference scryd-spec-v0.2.0.md §3 Out for "automatic migration of the v0.1.0 index" being out-of-scope.
  9. **v0.2.x → v0.2.(x+1) upgrade.** Explain that `sudo ./install.sh` is idempotent on a v0.2.x host: config and weights and index are preserved; binary and unit are replaced; daemon restarts.
  10. **Multi-user host.** One paragraph: deferred to v0.3.0; v0.2.0 is single-operator per host; if you need multiple operators today, run multiple Linux hosts or VMs.
  11. **glibc requirement.** The release tarball requires `glibc >= 2.34` (Ubuntu 22.04+, Debian 12+, Fedora 36+, Arch). Older distros need to build from source.

## Acceptance criteria
- [ ] `test -f ops/README.install.md`.
- [ ] `grep -F 'sudo ./install.sh' ops/README.install.md` matches the quick-install block.
- [ ] `grep -F 'sudo ./uninstall.sh' ops/README.install.md` matches the uninstall section.
- [ ] `grep -F '--remove-v01-data' ops/README.install.md` matches the migration section.
- [ ] `grep -F 'sudo scryd add-account' ops/README.install.md` matches the daily-use section.
- [ ] `grep -F 'sudo scryd rotate-password' ops/README.install.md` matches.
- [ ] `grep -F 'sudo scryd remove-account' ops/README.install.md` matches.
- [ ] `grep -F 'journalctl -u scryd' ops/README.install.md` matches (no `--user` form).
- [ ] `! grep -F 'journalctl --user -u scryd' ops/README.install.md` (no v0.1.0 user-form recipe leaks).
- [ ] `grep -F 'sudo systemctl restart scryd' ops/README.install.md` matches.
- [ ] `grep -E 'Isolation' ops/README.install.md` matches the section heading.
- [ ] `grep -F '/etc/scryd/config.toml' ops/README.install.md` matches.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
