Parent spec: [scryd v0.2.0](../../scryd-spec-v0.2.0.md)

# scryd v0.2.0 — migration

## §1 Summary

Owns the two version-boundary flows v0.2.0 has to handle: the v0.1.0 → v0.2.0 transition (not data-preserving — index dropped, fresh resync from IMAP) and the v0.2.x → v0.2.(x+1) upgrade (preserves config, weights, and index because the daemon's owned files survive an in-place re-install). Implementation work for both lives inside `ops/install.sh`; this slice fixes the contract, the operator-facing messages, and the test surface that proves the transitions behave as the spec promises.

## §2 Codebase reconnaissance

- v0.1.0 install layout the transition removes: per-user, files at `~/.local/bin/scryd`, `~/.local/bin/scryd-fetch-weights`, `~/.config/systemd/user/scryd.service`, `~/.config/scryd/config.toml`, `~/.local/share/scryd/` (data + assets). systemd user unit enabled via `systemctl --user enable --now scryd` (per `ops/install.sh:91-104`).
- Existing install smoke harness at `tests/install_sh.sh` — runs against a temp HOME with stub binaries and the skip env vars. Builds the fixture bundle in a tempdir (line 19-38), calls install.sh against `FAKE_HOME`, asserts FS layout (line 50-86). The pattern extends: a v0.1.0 install simulator can be a sibling fixture before invoking v0.2.0 install.sh.
- Existing flag parsing in install.sh — bash long-option style without getopt; the `--remove-v01-data` flag added in v0.2.0 follows the same pattern.
- `getent passwd` is available on every Debian / Ubuntu / Fedora / Arch install — used by build-and-packaging slice §3 Decision 10 for v0.1.0 detection across users.
- v0.2.x → v0.2.(x+1) re-install: install.sh is already idempotent for the system-user creation step (build-and-packaging slice settled-by-recon block); idempotency for the rest of the install — directories, unit, tmpfiles drop-in, binaries — needs explicit verification.

## §3 Decisions

1. **Confirmation flag name.** Options: `--remove-v01-data`, `--confirm-fresh-install`, `--force`. **Chosen:** `--remove-v01-data`. Rationale: names the user-visible side effect; `--force` is too generic and historically operators have learned to fear it without it being clear what gets forced.
2. **What gets removed during v0.1.0 → v0.2.0.** Options: full per-user removal (binaries, units, config, data, weights), only daemon-side (binaries + units, preserve config + data), nothing. **Chosen:** full per-user removal. Rationale: the spec's "fresh reindex" decision (in §3 Out) means the v0.1.0 data has no role going forward; leaving config behind would invite confusion when the operator runs `sudo scryd add-account` and writes to a different file.
3. **Detection coverage.** Options: probe per-user paths via `getent passwd` for UIDs ≥ 1000, probe only the calling user's home, probe `scryd --version` on `PATH`. **Chosen:** probe per-user paths via `getent passwd`. Rationale: same as build-and-packaging Decision 10 — multi-user hosts with v0.1.0 installed for several users get every instance caught and removed.
4. **v0.2.x → v0.2.(x+1) upgrade flow.** Options: re-running `install.sh` is idempotent and preserves operator state, separate `upgrade.sh`, version-aware install.sh that detects an existing v0.2.x install and takes a different code path. **Chosen:** re-running `install.sh` on a v0.2.x host is idempotent and preserves operator state. Rationale: simplest; one entrypoint; the operator doesn't have to know which command applies.
5. **What gets preserved across v0.2.x re-install.** Options: config + weights + index, config + weights only (force reindex), nothing. **Chosen:** config + weights + index. Rationale: matches the spec's "preserving my account configuration, my index, and my downloaded weights" user story.
6. **Operator-facing messages.** Options: terse one-liner with the flag name, multi-line explanation listing every detected v0.1.0 path. **Chosen:** multi-line list of detected paths plus the explanation and the flag. Rationale: operator may not remember they installed v0.1.0; surfacing the paths makes the choice informed.
7. **Test surface.** Options: integration test in `tests/install_sh.sh` that simulates v0.1.0 install then v0.2.0 over it; static check that install.sh contains the right command literals; manual pre-tag check only. **Chosen:** integration test in `tests/install_sh.sh`. Rationale: the migration is the v0.2.0 thing operators are most likely to encounter and most likely to be hurt by if it's broken; static checks miss `useradd` flag drift; manual gates rot.

## §4 Contracts & shapes

### `--remove-v01-data` semantics

When the v0.2.0 `install.sh` detects v0.1.0 artifacts on the host:

- **Without `--remove-v01-data`:** print every detected path (per detected user, file by file), print the explanation block: "scryd v0.2.0 ships a different on-disk layout than v0.1.0. Migration is not data-preserving — your indexed messages will be re-fetched from the IMAP server. Re-run with --remove-v01-data to proceed; existing v0.1.0 install is left untouched.", exit code 2, do nothing.
- **With `--remove-v01-data`:** for each detected user, in order:
  - `sudo -u <user> systemctl --user stop scryd 2>/dev/null || true`
  - `sudo -u <user> systemctl --user disable scryd 2>/dev/null || true`
  - `rm -f <home>/.local/bin/scryd <home>/.local/bin/scryd-fetch-weights`
  - `rm -f <home>/.config/systemd/user/scryd.service`
  - `rm -rf <home>/.config/scryd <home>/.local/share/scryd`
  - one log line: `scryd install: removed v0.1.0 install for user <user>`
  Then proceed with the v0.2.0 install (the rest of the build-and-packaging slice's sequence). The v0.2.0 daemon starts with an empty config and an empty index; the operator runs `sudo scryd add-account` afresh.

### `getent passwd`-based v0.1.0 detection

A bash function `detect_v01_users()` in `install.sh`:

- Iterates `getent passwd | awk -F: '$3 >= 1000 && $3 < 65534 {print $1":"$6}'` (UID ≥ 1000 and excluding `nobody`).
- For each `<user>:<home>`, tests the existence of any of: `<home>/.local/bin/scryd`, `<home>/.local/bin/scryd-fetch-weights`, `<home>/.config/systemd/user/scryd.service`, `<home>/.config/scryd/config.toml`.
- If any exist, append `<user>:<found-paths>` to a global array.
- After iteration, the array is non-empty iff any v0.1.0 install was found.

The function is pure-detection; the act-on-detection logic is separate and gated by `--remove-v01-data`.

### v0.2.x → v0.2.(x+1) upgrade flow

Re-running `install.sh` on a host that already has v0.2.x installed:

- **System user creation:** `getent passwd scryd >/dev/null` skips `useradd` (already idempotent in v0.2.0).
- **Directory creation:** `install -d -m 0700 -o scryd -g scryd /etc/scryd` is idempotent — `install -d` does not error when the dir exists with the right mode/owner.
- **Config file:** `install -m 0600 -o scryd -g scryd /dev/null /etc/scryd/config.toml` would truncate; install.sh skips this step if the file already exists. Existing operator-supplied accounts are preserved.
- **Binary install:** `install -m 0755 ./scryd /usr/local/bin/scryd` overwrites the previous binary atomically (via the install command's rename semantics). The running daemon is using the old text segment; the new binary takes effect on the next service restart.
- **Unit + tmpfiles drop-in:** rendered fresh from the new tarball's `.in` templates, written via `install`. Overwrites the previous version. `systemctl daemon-reload` picks up the changed unit.
- **Weights file:** `scryd-fetch-weights` checks the SHA-256 against the file at the target. If the file matches, the helper exits without re-downloading. If a v0.2.(x+1) bumps the pinned weights hash, the helper re-downloads.
- **Service restart:** `systemctl restart scryd` at the end of upgrade install picks up the new binary and unit.

The operator's config, index, and weights are preserved across the upgrade because they live at the same paths and are not touched.

### Failure modes

- v0.1.0 detected but `--remove-v01-data` not passed → exit 2, no changes (per build-and-packaging Decision §3 D10).
- v0.1.0 detected and removal fails on one user (e.g., systemctl --user can't be invoked because the user isn't logged in / no DBUS session) → install.sh logs a warning and continues with the file removal; the user's units may end up in a "leftover" state that they manually clean up via `loginctl terminate-user <user>` or by logging out.
- v0.2.x re-install on a host where `/etc/scryd/config.toml` is corrupt (operator hand-edited it) → preserved, daemon's `Config::load` fails (per security slice's preflight), systemd marks the service failed, operator sees the failure in `journalctl -u scryd` and edits the file.
- v0.2.x re-install during a running sync → systemd's `restart` stops the in-flight sync; the daemon's drainer state is durable in SQLite (per scryd-storage slice from v0.1.0); the next start replays unprocessed queue rows.

### Test additions

`tests/install_sh.sh` grows two scenarios on top of the existing v0.2.0 fresh-install case:

1. **`v0.1.0 → v0.2.0 transition test`** — inside the privileged docker container, simulate a v0.1.0 install for two test users (alice + bob): `sudo -u alice install -m 0755 fixture-scryd /home/alice/.local/bin/scryd` plus the unit and config files. Then run `./install.sh --user alice` (the v0.2.0 installer) without `--remove-v01-data`. Assert exit code 2, assert stderr contains both `/home/alice/.local/bin/scryd` and `/home/bob/.local/bin/scryd`, assert no v0.2.0 artifacts were created (`/etc/scryd/`, `/var/lib/scryd/`, the system unit, the daemon user). Then re-run with `--remove-v01-data`, assert exit 0, assert all v0.1.0 paths are gone, assert v0.2.0 layout is present.
2. **`v0.2.x re-install preserves operator state test`** — run `./install.sh --user alice --skip-weights`. Write a synthetic config to `/etc/scryd/config.toml` (`[[accounts]] id="testacct" host="imap.example.com" ...`). Run `./install.sh --user alice --skip-weights` again (the upgrade case). Assert the config file's mtime did not change (or its content is unchanged — mtime is risky in fast tests). Assert the daemon user was not deleted and recreated. Assert the `xtr-weights.gguf` file (if present from the test setup) is untouched.

Both scenarios run inside the same `debian:bookworm` privileged container the build-and-packaging slice introduced.

## §5 Sequence

### v0.1.0 → v0.2.0 (operator's first encounter)

1. Operator on a host running v0.1.0 downloads the v0.2.0 tarball and extracts.
2. Operator runs `sudo ./install.sh`.
3. install.sh runs `detect_v01_users()`. Returns `[alice:/home/alice/.local/bin/scryd /home/alice/.config/scryd/config.toml]` (or whatever it found).
4. install.sh prints the multi-line message naming alice's paths, the explanation, and `--remove-v01-data`. Exits 2.
5. Operator reads, decides to proceed, runs `sudo ./install.sh --remove-v01-data`.
6. install.sh re-runs `detect_v01_users()` (the host hasn't changed). For each detected user, stops the user-systemd unit (best-effort), deletes the binary, the unit file, the XDG dirs.
7. install.sh proceeds with the build-and-packaging sequence — system user, dirs, templates, tmpfiles, weights, daemon-reload, enable --now.
8. install.sh prints the next-steps block: `sudo scryd add-account`, `journalctl -u scryd -f`.
9. Operator runs `sudo scryd add-account`, supplies the same IMAP credential they had before, runs `sudo systemctl restart scryd`. The daemon resyncs the entire mailbox from the IMAP server. Time to first searchable message: minutes for a small mailbox, hours for a large one.

### v0.2.x → v0.2.(x+1) (operator's routine update)

1. Operator on a v0.2.x host downloads the v0.2.(x+1) tarball and extracts.
2. Operator runs `sudo ./install.sh`. (No flag needed — there's no v0.1.0 install to detect.)
3. `detect_v01_users()` returns empty.
4. install.sh runs the build-and-packaging sequence with idempotency: skip useradd (exists), skip dir creation (exists), skip config-file creation (exists, has operator data), overwrite binaries, re-render unit + tmpfiles, run `systemd-tmpfiles --create` (re-applies perms), run `scryd-fetch-weights` (no-op if SHA matches), `systemctl daemon-reload`, `systemctl restart scryd`.
5. Daemon comes back up under the new binary; reads the same config.toml; resumes from the same SQLite + witchcraft index; serves search.

## §6 Out of scope

- Index migration tooling (a `scryd migrate-v01` subcommand that copies messages from v0.1.0's data dir into v0.2.0's). Spec §3 Out explicitly excludes this.
- A v0.2.x → v0.3.0 upgrade story. Future spec.
- Backup/restore of the daemon's data dir. Operator-level concern (`tar -cf scryd-backup.tar /var/lib/scryd`).
- A `--keep-v01-data` mode that preserves the v0.1.0 install alongside the v0.2.0 one. Two scryd binaries on PATH would collide.
- Auto-detection of partial / corrupted v0.1.0 installs (e.g., binary present but no unit). The detection is best-effort; if any of the four probe paths is present, the host is considered "v0.1.0-installed" and the operator must opt into removal.

## §7 Open questions

- Whether the `--remove-v01-data` removal should also stop a running v0.1.0 daemon's lingering connection to the IMAP server (the user-systemd-stop call covers this if `loginctl enable-linger` was set; if the daemon is running under a non-lingering user-systemd that stopped at logout, there's no process to kill). Default position: best-effort `systemctl --user stop` is sufficient; if the v0.1.0 daemon is still running somehow, `pkill -u <user> scryd` is a fallback the operator can run manually.
- Whether v0.2.x → v0.2.(x+1) should refuse to proceed if the schema version of the SQLite store has advanced (i.e., the new daemon would read an older schema). Default position: out of scope until v0.2.0 actually has a schema-version-bump release; storage migrations are owned by the storage slice.

> If the parent spec is ambiguous on anything this slice depends on, stop and update the spec. Do not invent behavior here.
