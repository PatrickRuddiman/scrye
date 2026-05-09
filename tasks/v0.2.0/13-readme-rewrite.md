Parent slice: [scryd v0.2.0 — build-and-packaging](../../slices/0.2.0/build-and-packaging.md)
Depends on: 12

# Task 13 — readme-rewrite

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Update the top-level `README.md` to reflect v0.2.0: drop the macOS section, replace per-user install instructions with the sudo system installer, add a brief "Isolation properties" subsection, drop the v0.1.0 status banner.

## Tasks
- [ ] In `README.md`, replace the existing "Install" section with a v0.2.0 single-block Linux install:
  - One paragraph naming "v0.2.0 ships Linux x86_64 / aarch64 tarballs at the [Releases page](https://github.com/PatrickRuddiman/scrye/releases)."
  - One code-block showing `tar -xzf ...; cd ...; sudo ./install.sh`.
  - One paragraph linking to `ops/README.install.md` for the full walkthrough (sudo install, isolation properties, uninstall, migration).
  - Remove the `### macOS` subsection and the `~/.local/bin/` per-user instructions entirely.
- [ ] After the install section, add a new `## Isolation` subsection (3–4 sentences) explaining the v0.2.0 property: the daemon runs as a dedicated `scryd` system user, the config file is owned by that user mode 0600, agents running as the operator's UID cannot open it, the operator connects to the daemon over a Unix socket whose group permits only the operator. Link to `ops/README.install.md` and `scryd-spec-v0.2.0.md` for detail.
- [ ] Remove the v0.1.0 status note (the banner that read "v0.1.0 binaries on the GitHub Releases page implement the simpler `~/.local/bin/` per-user install ... v0.2.0 will land the dedicated-UID install"). v0.2.0 is the dedicated-UID install; the note is obsolete.
- [ ] Update the `## Status` section's last paragraph (or equivalent) so it no longer references "the live IMAP connection + daemon orchestration are in progress" if that's been resolved by the time this task ships, or leaves it in place if those tasks (12–16 from v0.1.0) are still deferred. Be honest either way.
- [ ] Verify the architecture diagram still reflects the v0.2.0 model. Update if it shows per-user paths.

## Acceptance criteria
- [ ] `test -f README.md`.
- [ ] `grep -F 'sudo ./install.sh' README.md` matches.
- [ ] `grep -E '^## Isolation' README.md` matches the new subsection heading.
- [ ] `! grep -F '~/.local/bin' README.md` (no per-user install instructions).
- [ ] `! grep -E '^### macOS' README.md` (no macOS section).
- [ ] `! grep -F 'v0.2.0 will land' README.md` (the v0.1.0 status banner is gone).
- [ ] `grep -F 'PatrickRuddiman/scrye' README.md` matches the Releases link.
- [ ] `grep -F 'ops/README.install.md' README.md` matches the cross-link.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
