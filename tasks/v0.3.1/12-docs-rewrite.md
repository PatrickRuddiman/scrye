Parent plan: scryd v0.3.1 — service pivot
Depends on: 01, 02, 03, 04, 05, 06, 07, 08, 09, 10, 11

# Task 12 — docs-rewrite

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Rewrite the project's prose under the v0.3.1 service framing. The README, install doc, spec, and inline doc-comments all carry residual "operator's UID can't read the file" + "single-operator host" language that the architectural unwind contradicts. Add a fresh `docs/security.md` documenting the open-API posture and the consumer's responsibility to layer auth.

## Tasks
- [ ] Rewrite `README.md`:
  - Top-of-file framing: "scryd is a service. Install once, link N IMAP accounts, query the open search API filtered by account_ids."
  - Drop the "Isolation" section's framing about operator UIDs reading config.toml. Replace with a "What scryd defends against / doesn't" paragraph linking to `docs/security.md`.
  - "Install" section drops the `--user` examples and the v0.1.0→v0.2.0 migration block.
  - "Use" section adds `--accounts foo,bar` examples; `scryd sync` and `scryd status` get one-line entries each.
  - Drop the operator-vs-daemon-UID architecture diagram caption; replace with "scryd is a service; access control lives in the consumer's API layer, not here".
- [ ] Rewrite `ops/README.install.md`:
  - Drop "Isolation properties" table.
  - Drop "v0.1.0 → v0.2.0 migration" section.
  - Drop "Multi-user host" paragraph.
  - "First account" becomes `sudo scryd add-account` (no restart hint — task 10 makes the daemon auto-reconcile).
  - "Daily commands" loses the per-mutation restart hint.
- [ ] Rename `scryd-spec-v0.2.0.md` → `scryd-spec.md` via `git mv`. Rewrite §1 (positioning under service framing), §2 (personas: operator who owns the server, consumer who builds the higher-layer API, end-user who never sees scryd directly), §3 In/Out (drop the "credential isolation from operator UID" entries; add "open API by default, account_ids filter for caller-driven scoping" as the new §3 In headline; keep OAuth, attachments, GUI, sending, cross-host, macOS/Windows in §3 Out), §4 (drop the "operator's account cannot recover the credential" property; new property: "the consumer's API layer is the only access-control boundary; scryd does not enforce per-end-user filtering").
- [ ] Create `docs/security.md` (new file) with:
  - "What scryd defends against": IMAP credential at rest is owned by the scryd system user; messages are read-only against IMAP; no execution of received content.
  - "What scryd does NOT defend against": end-user RBAC, account-scope enforcement, network-level access to the socket, theft of `/etc/scryd/config.toml` by a host-root attacker.
  - "Your job as a consumer": authenticate end-users at your API; pass `account_ids` to scope each query; never expose scryd's socket directly.
  - Cross-link to README and `ops/README.install.md`.
- [ ] Inline doc cleanup:
  - `crates/scryd-runtime/src/lib.rs:5-9` — already touched in task 06; ensure it reflects the wired-up state.
  - `crates/scryd-imap/src/scheduler.rs:134-145` — task 09 deletes the placeholder bodies; ensure the doc-comments around `reconcile`/`request_pass` describe their behaviour, not their absence.
  - `crates/scryd-api/src/peercred.rs` and `socket.rs` — drop "operator's UID" / "single-operator host" framings; rephrase as "kernel-level access gate, off by default per [server] require_peer_uid".
  - `ops/README.install.md` and the install.sh comment header.
- [ ] Create `tasks/v0.3.1/triage/` with seven short markdown notes (one per item from the plan's "Out of scope (triage docs only)" list). Each is 5-15 lines: title, why deferred, what triggers picking it up.

## Acceptance criteria
- [ ] `test -f docs/security.md`.
- [ ] `test -f scryd-spec.md && ! test -f scryd-spec-v0.2.0.md`.
- [ ] `! grep -F 'operator' README.md | grep -iE 'uid|user' | head -1` returns no match (or only matches in deliberate "the operator runs sudo …" senses; manual review the few matches before checking the box).
- [ ] `! grep -F 'v0.1.0 → v0.2.0' ops/README.install.md`.
- [ ] `grep -F 'account_ids' README.md` matches.
- [ ] `grep -F 'scryd sync' README.md` matches.
- [ ] `grep -F 'scryd status' README.md` matches.
- [ ] `grep -F 'docs/security.md' README.md` matches.
- [ ] `ls tasks/v0.3.1/triage/*.md | wc -l` returns at least 7.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
