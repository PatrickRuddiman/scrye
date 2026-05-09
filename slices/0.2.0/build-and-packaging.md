Parent spec: [scryd v0.2.0](../../scryd-spec-v0.2.0.md)

# scryd v0.2.0 — build-and-packaging

## §1 Summary

Owns how v0.2.0 installs onto a Linux host, where the daemon's owned files live, how the systemd unit is rendered per install, how `/run/scryd/` is provisioned across reboots, how the operator uninstalls, how the CI release matrix and tarball assembly change, and how a v0.1.0 install on the same host is detected and removed during a v0.2.0 install. Reverses build-and-packaging slice §3 Decisions 7 + 8 from v0.1.0 (per-user `~/.local/bin` install + user systemd unit) and replaces them with a system install rooted at FHS paths under a dedicated daemon account.

## §2 Codebase reconnaissance

- v0.1.0 per-user installer at `ops/install.sh` (105 lines, bash). Refuses-AS-root, copies bundle from script-dir, mode-0700 dirs under XDG paths, runs `scryd-fetch-weights`, calls `systemctl --user daemon-reload`. Test-only env vars: `SCRYD_INSTALL_SKIP_SYSTEMCTL`, `SCRYD_INSTALL_SKIP_WEIGHTS`, `SCRYD_INSTALL_ALLOW_ROOT`.
- v0.1.0 user systemd unit at `ops/scryd.service` (24 lines). `Type=simple`, `ExecStart=%h/.local/bin/scryd serve`, `[Install] WantedBy=default.target`. Hardening: `NoNewPrivileges=yes`, `ProtectSystem=strict`, `ProtectHome=read-only`, `ReadWritePaths=%h/.local/share/scryd %h/.config/scryd %t/scryd`, `PrivateTmp=yes`, `PrivateDevices=yes`, `LockPersonality=yes`, `RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX`, `SystemCallFilter=@system-service`, `SystemCallArchitectures=native`. No `MemoryDenyWriteExecute` (witchcraft JIT conflict).
- Weights helper at `crates/scryd-fetch-weights/src/main.rs`. clap `--target <dir>` defaults to `$XDG_DATA_HOME/scryd/assets/`; `--url`, `--sha256` overrides. Refuses to run as root (overridable by `SCRYD_FETCH_WEIGHTS_ALLOW_ROOT` for tests). Atomic write at mode 0600.
- Install smoke harness at `tests/install_sh.sh`. Builds a stub bundle with fake scryd / scryd-fetch-weights / scryd.service, runs install.sh against a temp HOME with skip env vars set, asserts FS layout + modes + content survival.
- CI release pipeline at `.github/workflows/release.yml`. Matrix builds Linux x86_64 / aarch64 (cross) + macOS x86_64 / aarch64; package step assembles `scryd-${tag}-${arch}-${os}.tar.gz` containing binaries + LICENSE + (Linux only) the systemd unit + install.sh + README.install.md.
- Path resolution at `crates/scryd-runtime/src/xdg.rs` (54 lines). Pure env-driven from `XDG_RUNTIME_DIR`, `XDG_CONFIG_HOME` (with `HOME` fallback), `XDG_DATA_HOME` (with `HOME` fallback). Setting these in a systemd `Environment=` drives the daemon to any layout with no Rust change.
- Workspace `Cargo.toml` `[workspace.package]` declares `version = "0.1.0"`, `publish = false`. v0.2.0 means bumping `version` here.
- Release-process docs at `ops/README.release.md` and operator install docs at `ops/README.install.md`. Both v0.1.0-flavored; both rewritten by this slice.
- No existing analog for: system-user creation via `useradd --system`, `/etc/tmpfiles.d/` drop-ins, system-wide `/etc/systemd/system/` unit installation, an uninstall script, install-time unit templating.

## §3 Decisions

1. **Daemon-owned directory layout.** Options: FHS standard paths, single tree under `/var/lib/scryd/`, single tree under `/opt/scryd/`. **Chosen:** FHS standard — `/etc/scryd/` for config, `/var/lib/scryd/` for data + weights, `/run/scryd/` for the socket. Rationale: matches every other system daemon; the systemd unit drives `xdg.rs` via `Environment=` so no Rust change.
2. **Permission model for `/run/scryd/` and the socket.** Options: FS group + peercred, wide-open dir + peercred-only, group-on-socket-only. **Chosen:** FS group + peercred — `/run/scryd/` mode 0750 owned `scryd:<calling-user>`, socket mode 0660 owned `scryd:<calling-user>`. Rationale: meets the spec's "no information about whether the operator runs scryd" bar for unrelated Linux users; peercred is defense-in-depth.
3. **`/run/scryd/` lifecycle across reboots.** Options: `tmpfiles.d` drop-in, `RuntimeDirectory=` in the unit, `ExecStartPre=` in the unit. **Chosen:** `tmpfiles.d` drop-in — install.sh writes `/etc/tmpfiles.d/scryd.conf` declaring `/run/scryd/` mode 0750 owner `scryd` group `<calling-user>`, then runs `systemd-tmpfiles --create` once. Rationale: `RuntimeDirectory=` doesn't support a non-daemon group; `ExecStartPre=` interleaves shell into the unit.
4. **Operator discovery at install time.** Options: `$SUDO_USER` default + `--user` flag, mandatory `--user`, interactive prompt fallback. **Chosen:** `$SUDO_USER` default + `--user` override. Rationale: matches Linux installer convention; `--user` covers the `sudo -u root ./install.sh` edge case.
5. **Unit and tmpfiles drop-in templating.** Options: render at install time, static unit + drop-in, template unit (`scryd@.service`). **Chosen:** render at install time — ship `ops/scryd.service.in` and `ops/scryd.tmpfiles.in` in the tarball; install.sh `sed`-substitutes `__UID__` (allowed peer UID) and `__USER__` (calling user's name) and writes the rendered files to `/etc/systemd/system/scryd.service` and `/etc/tmpfiles.d/scryd.conf`. Rationale: simplest; no override-file machinery; template units would over-engineer a single-tenant install.
6. **scryd-fetch-weights identity at install.** Options: `sudo -u scryd` from install.sh, run as root then chown, defer to first daemon start via `ExecStartPre=`. **Chosen:** `sudo -u scryd /usr/local/bin/scryd-fetch-weights --target /var/lib/scryd/assets/` from install.sh. Rationale: file ownership correct on first try; matches the helper's existing refuse-as-root posture; install fails fast if the network is unreachable instead of leaving a daemon stuck in failed-to-start.
7. **Uninstall mechanism.** Options: separate `ops/uninstall.sh`, `install.sh --uninstall`, `scryd uninstall` subcommand. **Chosen:** separate `ops/uninstall.sh` shipped in the tarball. Rationale: clear intent; operator can read it before running; doesn't conflate with install.
8. **Install smoke test in CI.** Options: privileged docker container, manual pre-tag check, static-analysis-only. **Chosen:** privileged docker container — extend `tests/install_sh.sh` to run inside `debian:bookworm` with appropriate caps on the GitHub Actions runner. Rationale: install is the v0.2.0 thing we most want to verify; static analysis can't catch a typo in `useradd` flags or a tmpfiles drop-in that doesn't parse.
9. **Reload after config-mutating CLI commands.** Options: operator runs `sudo systemctl restart scryd`, CLI auto-runs `systemctl restart scryd`, implement SIGHUP in the daemon. **Chosen:** operator runs `sudo systemctl restart scryd`; the CLI prints the hint after a successful write. Rationale: SIGHUP needs scryd-runtime work that's out of this slice; auto-restart shells out fragilely.
10. **v0.1.0 install detection.** Options: probe per-user paths via `getent passwd`, probe only `$SUDO_USER`, probe `scryd --version` on `PATH`. **Chosen:** walk `getent passwd` for users with UID ≥ 1000, check each home for `~/.local/bin/scryd` or `~/.config/systemd/user/scryd.service`. Rationale: catches multi-user hosts where v0.1.0 was provisioned for several operators; a single-user probe leaves orphan v0.1.0 daemons running; `scryd --version` gives false positives from leftover binaries.
11. **Binary install location.** Options: `/usr/local/bin/` for both, split bin/sbin, `/opt/scryd/bin/`. **Chosen:** `/usr/local/bin/scryd` and `/usr/local/bin/scryd-fetch-weights`. Rationale: FHS-standard, on every operator's default `PATH`; the helper is a public-model downloader and doesn't need to be hidden.
12. **Release matrix.** Options: Linux only, keep macOS, add musl static. **Chosen:** Linux only — drop the macOS jobs from `.github/workflows/release.yml`. Rationale: spec §3 Out includes any OS other than Linux; macOS binaries with no install path are noise; musl is a future hardening if older-glibc complaints arrive.

Settled by recon, recorded for completeness:
- Daemon Linux account name is `scryd`. Created via `useradd --system --shell /usr/sbin/nologin --home /var/lib/scryd scryd` (idempotent — install.sh skips if already present).
- Workspace `[workspace.package].version` bumps from `0.1.0` to `0.2.0`.
- Weights file at `/var/lib/scryd/assets/xtr-weights.gguf` mode 0644 (readable by anyone). Model weights are public; restricting access is theater.
- Tarball shape unchanged in spirit; v0.2.0 contents enumerated in §4.

## §4 Contracts & shapes

### Filesystem layout produced by a successful install

- `/usr/local/bin/scryd` — operator CLI + daemon. Mode 0755, owner `root:root`.
- `/usr/local/bin/scryd-fetch-weights` — install-time helper. Mode 0755, owner `root:root`.
- `/etc/scryd/` — directory. Mode 0700, owner `scryd:scryd`.
- `/etc/scryd/config.toml` — daemon-owned config. Mode 0600, owner `scryd:scryd`. Empty after a fresh install; populated by `sudo scryd add-account`.
- `/var/lib/scryd/` — directory. Mode 0700, owner `scryd:scryd`.
- `/var/lib/scryd/assets/` — directory. Mode 0755, owner `scryd:scryd`.
- `/var/lib/scryd/assets/xtr-weights.gguf` — T5 weights. Mode 0644, owner `scryd:scryd`.
- `/run/scryd/` — directory. Mode 0750, owner `scryd:<calling-user>`. Created by `systemd-tmpfiles --create` from the drop-in below; recreated each boot.
- `/run/scryd/scryd.sock` — UDS. Mode 0660, owner `scryd:<calling-user>`. Created by the daemon at start.
- `/etc/systemd/system/scryd.service` — system unit, rendered from `ops/scryd.service.in`.
- `/etc/tmpfiles.d/scryd.conf` — single-line drop-in: `d /run/scryd 0750 scryd <calling-user> -`.

### `scryd.service.in` template (intent, not literal)

- Section `[Unit]`: `Description`, `After=network-online.target`, `Wants=network-online.target`.
- Section `[Service]`:
  - `Type=simple`
  - `User=scryd`, `Group=scryd`
  - `ExecStart=/usr/local/bin/scryd serve`
  - `Restart=on-failure`, `RestartSec=5`
  - `Environment=SCRYD_ALLOWED_UID=__UID__`
  - `Environment=XDG_CONFIG_HOME=/etc/scryd`
  - `Environment=XDG_DATA_HOME=/var/lib/scryd`
  - `Environment=XDG_RUNTIME_DIR=/run/scryd`
  - `NoNewPrivileges=yes`, `ProtectSystem=strict`, `ProtectHome=true`
  - `ReadWritePaths=/etc/scryd /var/lib/scryd /run/scryd`
  - `PrivateTmp=yes`, `PrivateDevices=yes`, `LockPersonality=yes`
  - `RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX`
  - `SystemCallFilter=@system-service`, `SystemCallArchitectures=native`
- Section `[Install]`: `WantedBy=multi-user.target`.

`ProtectHome` tightens from `read-only` (v0.1.0) to `true` because the daemon never reads `/home` in v0.2.0. `WantedBy` changes from `default.target` (user manager) to `multi-user.target` (system manager).

### `scryd.tmpfiles.in` template

One line: `d /run/scryd 0750 scryd __USER__ -`.

### `ops/install.sh` CLI

- Refuses to run if `EUID != 0` (the v0.1.0 polarity is reversed).
- Flags:
  - `--user <name>` — operator login. Defaults to `$SUDO_USER`. Resolved to UID via `getent passwd <name>`.
  - `--remove-v01-data` — explicit confirmation that the script may stop and remove a detected v0.1.0 install.
  - `--skip-weights` (test/offline) — skip the `scryd-fetch-weights` step.
  - `--skip-systemctl` (test) — skip `daemon-reload` / `enable --now`.
- Idempotent: re-running on a clean v0.2.0 install is a no-op (system user exists, dirs exist with correct perms, files are reinstalled at the same paths).
- Exit codes: 0 success, 1 missing/refused preconditions, 2 v0.1.0 detected without `--remove-v01-data`.
- Stdout: progress lines naming each step. Stderr: warnings + errors only.

### `ops/uninstall.sh` CLI

- Refuses to run if `EUID != 0`.
- Flags: none in v0.2.0 (a `--dry-run` was considered and explicitly out-of-scope per spec §3).
- Stops `scryd.service`, disables it, removes the unit file + drop-in + tmpfiles drop-in, removes `/etc/scryd`, `/var/lib/scryd`, `/run/scryd`, removes `/usr/local/bin/scryd` and `/usr/local/bin/scryd-fetch-weights`, runs `userdel scryd`. Each step is idempotent (skip if absent). Exit 0 on success.

### Tarball contents

`scryd-vX.Y.Z-<arch>-linux.tar.gz` extracts to a directory `scryd-vX.Y.Z-<arch>-linux/` containing:

- `scryd` — daemon + operator CLI
- `scryd-fetch-weights` — install-time helper
- `scryd.service.in` — systemd unit template
- `scryd.tmpfiles.in` — tmpfiles drop-in template
- `install.sh` — system installer (mode 0755)
- `uninstall.sh` — system uninstaller (mode 0755)
- `LICENSE`
- `README.install.md` — operator-facing install + uninstall doc

`<arch>` is `x86_64` or `aarch64`. macOS tarballs are not produced.

### CI release pipeline shape

`.github/workflows/release.yml` runs three stages on `push: tags: ['v*']`:

1. `license-check` — `cargo deny check` (existing; unchanged).
2. `build` — matrix `target: [x86_64-unknown-linux-gnu, aarch64-unknown-linux-gnu]`. `aarch64` job uses `cross`. Each job runs `cargo test --workspace` (native target only), then `cargo build --release`, then assembles the tarball + sha256 sidecar described above, then uploads workflow artifacts.
3. `release` — downloads artifacts, runs the new install smoke (see below), creates the GitHub release with the tarballs attached.

### Install smoke test in CI

`tests/install_sh.sh` is rewritten for v0.2.0. CI invokes it inside a `debian:bookworm` container started via `docker run --privileged --rm -v "$PWD:/workspace" -w /workspace debian:bookworm bash tests/install_sh.sh`. The harness:

- Creates a fixture bundle directory with stub `scryd` / `scryd-fetch-weights` (each a one-line bash script that prints `fake-scryd "$@"`), the `scryd.service.in` and `scryd.tmpfiles.in` templates from the repo, the real `install.sh`, the real `uninstall.sh`.
- Adds two test users (`useradd alice; useradd mallory`) inside the container.
- Runs `./install.sh --user alice --skip-weights` (skip weights — no internet inside the container; the fixture stub for `scryd-fetch-weights` is harmless to run, but skipping is cleaner).
- Asserts the FS layout from §4: directory existence, ownership (`stat -c '%U %G %a'`), file contents (the rendered unit contains `User=scryd`, `SCRYD_ALLOWED_UID=<alice's uid>`).
- Asserts `/run/scryd/` is mode 0750 owned `scryd:alice`.
- Asserts `cat /etc/scryd/config.toml` as `mallory` returns permission denied.
- Runs `./uninstall.sh` and asserts every file from the install is gone.

## §5 Sequence

1. **Operator downloads tarball and extracts.** `tar -xzf scryd-vX.Y.Z-x86_64-linux.tar.gz; cd scryd-vX.Y.Z-x86_64-linux`. No system change yet.
2. **Operator runs install.** `sudo ./install.sh` (or `sudo ./install.sh --user alice` to install for someone other than `$SUDO_USER`). install.sh reads `$SUDO_USER` (or `--user` value), resolves UID via `getent passwd`, and proceeds.
3. **install.sh checks for v0.1.0.** Iterates `getent passwd | awk -F: '$3 >= 1000 {print $6}'`. For each home dir, tests `~/.local/bin/scryd` and `~/.config/systemd/user/scryd.service`. If any are present and `--remove-v01-data` is not set: print every path found, exit 2 with a message naming the flag. If `--remove-v01-data` is set: per detected user, run `sudo -u <user> systemctl --user stop scryd` (best-effort), then `sudo -u <user> systemctl --user disable scryd` (best-effort), then `rm -f ~/.local/bin/scryd ~/.local/bin/scryd-fetch-weights ~/.config/systemd/user/scryd.service`, then `rm -rf ~/.config/scryd ~/.local/share/scryd`. v0.2.0 install proceeds with a fresh empty index.
4. **install.sh creates the daemon system user.** `getent passwd scryd >/dev/null || useradd --system --shell /usr/sbin/nologin --home /var/lib/scryd scryd`.
5. **install.sh creates daemon-owned directories.** `install -d -m 0700 -o scryd -g scryd /etc/scryd /var/lib/scryd`. `install -d -m 0755 -o scryd -g scryd /var/lib/scryd/assets`. `install -m 0600 -o scryd -g scryd /dev/null /etc/scryd/config.toml` (idempotent: only creates if missing).
6. **install.sh installs the binaries.** `install -m 0755 ./scryd /usr/local/bin/scryd; install -m 0755 ./scryd-fetch-weights /usr/local/bin/scryd-fetch-weights`.
7. **install.sh renders templates.** `sed -e "s/__UID__/$ALLOWED_UID/g" -e "s/__USER__/$ALLOWED_USER/g" scryd.service.in > /etc/systemd/system/scryd.service`. Same for `scryd.tmpfiles.in` → `/etc/tmpfiles.d/scryd.conf`.
8. **install.sh provisions `/run/scryd/`.** `systemd-tmpfiles --create /etc/tmpfiles.d/scryd.conf`. The dir exists immediately at the right perms; subsequent boots get the same dir from the same drop-in.
9. **install.sh fetches weights.** Unless `--skip-weights`: `sudo -u scryd /usr/local/bin/scryd-fetch-weights --target /var/lib/scryd/assets/`. The helper writes the GGUF file at mode 0600, which install.sh then `chmod`s to 0644 (or the helper grows a `--mode 0644` flag; pick one in implementation).
10. **install.sh enables and starts the daemon.** `systemctl daemon-reload; systemctl enable --now scryd`. Daemon comes up under `User=scryd`, reads its empty config, opens its UDS, idles.
11. **install.sh prints next-steps.** Lines naming `sudo scryd add-account`, `journalctl -u scryd -f`, `sudo systemctl restart scryd` (after config changes).
12. **Operator runs `sudo scryd add-account`.** The CLI (running as root via sudo) writes `/etc/scryd/config.toml`, succeeds. (CLI design owned by the cli slice.)
13. **Operator runs `sudo systemctl restart scryd`.** The daemon picks up the new config and starts indexing.
14. **Operator runs `scryd search "..."` from their normal account.** The CLI dials `/run/scryd/scryd.sock`; the daemon's peercred check accepts the operator's UID; the search returns.
15. **Operator decides to uninstall.** `sudo ./uninstall.sh` from the tarball directory (or operator keeps the tarball for this purpose). Daemon stopped, files removed, system user deleted. Exit 0.

## §6 Out of scope

- The peercred behavior change itself (the env-var read, the test additions). Owned by the security slice.
- The CLI elevation split (which verbs require sudo, what messages they print on failure). Owned by the cli slice.
- The v0.1.0 → v0.2.0 transition's user-data handling beyond removal. Index-preservation was decided "fresh reindex" in spec §3 Out and §2 acceptance criteria; this slice implements the removal, the migration slice owns the documentation of the operator-facing transition.
- Multi-tenant filtering, OAuth, OS keychains, non-Linux targets — all spec-level out-of-scope.
- A `.deb` or `.rpm` package. Tarball-only in v0.2.0; downstream packaging is a future ergonomic.
- Code-signing or Sigstore signatures on the tarball. Future hardening.
- Auto-update / built-in updater. Operator pulls a new tarball and re-runs install.sh; v0.2.x → v0.2.(x+1) preserves config and index because the daemon's owned files stay in place across re-install.

## §7 Open questions

- Whether the weights file's final mode is 0644 (set by install.sh after the helper writes it 0600) or whether `scryd-fetch-weights` grows a `--mode <octal>` flag so the helper writes the final mode in one step. Default position: install.sh `chmod`s after; revisit if the chmod creates a transient mode-0600 window long enough to matter (it does not, in practice).
- Whether the install smoke test in CI runs against a single Debian base image or matrix-tests across debian-bookworm, ubuntu-22.04, and fedora-39 to catch distro-specific `useradd` / `systemd-tmpfiles` flag drift. Default position: bookworm only for v0.2.0; expand if a real distro-specific bug bites.

> If the parent spec is ambiguous on anything this slice depends on, stop and update the spec. Do not invent behavior here.
