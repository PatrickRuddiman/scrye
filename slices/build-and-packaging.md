Parent spec: [scryd-spec.md](../scryd-spec.md)

# scryd — build-and-packaging

## §1 Summary

Owns how scryd is compiled, what its dependency pins are, what ships in the install artifact, where T5 weights come from at runtime, what the systemd user unit looks like, and how the per-architecture release matrix is produced. Every "decided in build-and-packaging" deferral from earlier slices lands here.

## §2 Codebase reconnaissance

> Greenfield: no existing system to reconcile. Decisions below are unconstrained.

External pieces this slice pins concretely:

- `dropbox/witchcraft` master, pinned to a specific commit hash, with the per-arch feature flags decided below. README documents `t5-quantized` (default), `t5-openvino`, `metal`, `fbgemm`, `hybrid-dequant`, `embed-assets`, `napi`, `progress`. Recommended sets: Apple Silicon `t5-quantized,metal`; Intel Mac `t5-quantized,fbgemm,hybrid-dequant`; Intel Windows `t5-openvino,fbgemm`. No explicit Linux line — we infer below.
- T5 XTR weights from Google DeepMind on Hugging Face, quantized to GGUF — Witchcraft ships Python scripts (`make warp-cli` runs them) that download and quantize.
- `rustls` for TLS — pure-Rust, Apache/MIT, static-friendly.
- `axum` 0.7+ on `tokio` 1.x.
- `htmd` for HTML→Markdown (the `mime-and-markdown` slice's deferred crate pin).
- `mail-parser`, `async-imap`, `serde`, `serde_json`, `clap` (derive), `rpassword`, `toml_edit`, `reqwest` (with `unix-socket` feature) — all named in earlier slices.
- `systemd --user` (≥ 242 for `ProtectHome=read-only` + `ReadWritePaths=` on user units).

## §3 Decisions

1. **Cargo workspace, one binary, several library crates.** Workspace at repo root. Members:
    - `scryd` — the binary, links everything; entry points are `cli`, `serve`, `add-account`, `reindex`, `search`.
    - `scryd-config` — the `config.toml` types and reader.
    - `scryd-storage` — `meta.sqlite` + raw store + work queue + migrations.
    - `scryd-mime` — MIME parsing + HTML→Markdown.
    - `scryd-imap` — IMAP client wrappers + scheduler.
    - `scryd-search` — Witchcraft integration + indexer task.
    - `scryd-api` — axum router and handlers.
    - `scryd-runtime` — process supervision, signals, `serve` orchestration; depends on every other library crate.
    - `scryd-cli` — clap parser + verb dispatchers; produces the binary `scryd` via `[[bin]]`.
   Rationale: matches the slice taxonomy 1:1 so each slice owns one crate; isolates compile-time impact when iterating; no FFI or C deps make a workspace cheap.
2. **Per-arch Witchcraft feature flag set.**
    - `x86_64-unknown-linux-gnu`: Witchcraft features `t5-quantized,fbgemm,hybrid-dequant`. Mirrors the README's Intel-Mac recommendation; the same FFI-quantization story works on x86_64 Linux.
    - `aarch64-unknown-linux-gnu`: Witchcraft features `t5-quantized` only. ARM has no `fbgemm` build (intel-specific); Witchcraft's `metal` feature is Apple-Silicon-only and unavailable on Linux ARM.
    Rationale: best per-arch perf; minimal build surface; aligns with what the README explicitly supports.
3. **TLS = `rustls`.** Pin `rustls` + `tokio-rustls` + the `webpki-roots` crate for trust anchors. No system OpenSSL build dep. Rationale: spec preferred this; reproducible static-ish builds; avoids the `libssl3` ABI dance across distros; webpki-roots is small enough to embed.
4. **T5 weights ship out-of-band, not embedded.** Witchcraft is built **without** the `embed-assets` feature. Weights live at `$XDG_DATA_HOME/scryd/assets/` on disk, fetched once by the install script. Rationale: embedding weights pushes the `scryd` binary past 500 MB; a per-user disk asset directory is cheap; upgrading the binary doesn't require re-downloading weights every release.
5. **HTML→Markdown crate = `htmd`.** Pinned to a specific version. Rationale: maintained, pure Rust, produces Markdown directly (mime-and-markdown's contract), tolerates the malformed HTML that arrives via email better than the older `html2md` crate.
6. **Single tarball install artifact per arch.**
    - Filename: `scryd-<version>-<arch>-linux.tar.gz`.
    - Contents: `scryd` (the binary), `scryd.service` (systemd user unit), `install.sh` (per-user install script), `LICENSE`, `README.install.md` (operator-facing installation steps).
    - Weights are NOT in the tarball; the install script downloads them on first run.
    Rationale: separates "the program" from "the model"; reproducible per-arch artifacts; easy to publish on GitHub Releases.
7. **Install model: per-user, no root required.** `install.sh` does:
    - Copy `scryd` to `~/.local/bin/scryd` (creates the dir if needed; warns if `~/.local/bin` is not in `$PATH`).
    - Copy `scryd.service` to `~/.config/systemd/user/scryd.service`.
    - If `$XDG_DATA_HOME/scryd/assets/` is missing, run a separate `scryd-fetch-weights` helper that downloads the GGUF weights file to that dir, mode `0600`. Helper is a small Rust binary built alongside `scryd` that does HTTPS with `rustls`, verifies a SHA-256 hash baked into its source, and fails loudly on mismatch.
    - Print next-steps: `systemctl --user daemon-reload; scryd add-account; systemctl --user enable --now scryd; loginctl enable-linger $USER`.
    Rationale: spec's "no system-wide privilege required"; the operator can install without bothering an admin; weights download is a clear separate step the operator sees.
8. **systemd user unit.** Concrete file shipped in the tarball:
    ```ini
    [Unit]
    Description=scryd — read-only IMAP indexer & search daemon
    After=network-online.target
    Wants=network-online.target

    [Service]
    Type=simple
    ExecStart=%h/.local/bin/scryd serve
    Restart=on-failure
    RestartSec=5

    NoNewPrivileges=yes
    ProtectSystem=strict
    ProtectHome=read-only
    ReadWritePaths=%h/.local/share/scryd %h/.config/scryd %t/scryd
    PrivateTmp=yes
    PrivateDevices=yes
    LockPersonality=yes
    RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX
    SystemCallFilter=@system-service
    SystemCallArchitectures=native

    [Install]
    WantedBy=default.target
    ```
   `%h` = the user's home; `%t` = `$XDG_RUNTIME_DIR`. Rationale: minimum-privilege per spec hardening directives; user-level Install target so the unit is for the user manager. `MemoryDenyWriteExecute=yes` is **not** included — Witchcraft's `fbgemm` and `t5-quantized` paths use `mmap`'d quantized weights and may use JIT-style code patches that conflict with `W^X`. Confirmed-safe MDWE is a §7 follow-up.
9. **Release profile.** `[profile.release]` in workspace root: `opt-level = 3`, `lto = "fat"`, `codegen-units = 1`, `strip = "symbols"`, `panic = "abort"`. Rationale: smallest binary that's also fastest; panic=abort is acceptable in a daemon that systemd will restart.
10. **CI matrix.** GitHub Actions; per-arch jobs:
    - `ubuntu-22.04` (`x86_64-unknown-linux-gnu`): `cargo build --release --target x86_64-unknown-linux-gnu`, packages tarball.
    - `ubuntu-22.04` with `aarch64-unknown-linux-gnu` cross-compile via `cross-rs` (or self-hosted ARM runner if available): same.
    - Linux glibc target chosen because `musl` adds toolchain pain for Witchcraft's `fbgemm` (C++ → glibc-flavored). v1 is glibc-only.
    - Each release tag pushes both tarballs to GitHub Releases.
    Rationale: cheap, reproducible, no Apple/Microsoft surface to worry about, spec is Linux-only.
11. **Weight provenance.** Witchcraft's upstream Python download script names a specific Hugging Face revision; we pin the same revision in our `scryd-fetch-weights` helper and bake the SHA-256 of the resulting GGUF file into the helper's source. Re-running the helper without the weight file is idempotent. Rationale: reproducible installs; supply-chain hygiene; easy to bump in a future release.
12. **No Debian / RPM / Homebrew / AUR / Nix packaging in v1.** Just tarballs on GitHub Releases. Rationale: out-of-scope for a v1 daemon that targets ops-savvy operators; downstream packaging is a v2 ergonomic.
13. **Workspace `Cargo.toml` hygiene.** All shared dependency versions live in `[workspace.dependencies]`; member crates inherit via `dep = { workspace = true }`. Rationale: one place to bump rust edition / toolchain / lockfile drift.

## §4 Contracts & shapes

Workspace layout (concrete paths):

```
scryd-spec.md
slices/…
Cargo.toml                         # workspace root
Cargo.lock
crates/
  scryd-config/
  scryd-storage/
  scryd-mime/
  scryd-imap/
  scryd-search/
  scryd-api/
  scryd-runtime/
  scryd-cli/
scryd/                             # the binary crate
  Cargo.toml                       # has [[bin]] name="scryd"
  src/main.rs
ops/
  scryd.service                    # systemd user unit (ships in tarball)
  install.sh                       # per-user install (ships in tarball)
  README.install.md
.github/workflows/release.yml      # CI matrix
```

Workspace `Cargo.toml` excerpts (intent, not literal code):

- `[workspace.package]`: `edition = "2021"`, `rust-version = "1.78"`, `license = "Apache-2.0"`, `repository = "<TBD>"`.
- `[workspace.dependencies]`: pinned versions for `tokio`, `axum`, `rustls`, `tokio-rustls`, `webpki-roots`, `rusqlite` (bundled feature so we statically link sqlite), `serde`, `serde_json`, `clap`, `rpassword`, `toml_edit`, `reqwest`, `mail-parser`, `htmd`, `async-imap`, `tracing`, `tracing-subscriber`.
- Witchcraft as a git dep: `witchcraft = { git = "https://github.com/dropbox/witchcraft", rev = "<commit-hash-pinned-at-coding-time>", default-features = false, features = ["t5-quantized", "fbgemm", "hybrid-dequant"] }` for the x86 build; `features = ["t5-quantized"]` via target-specific dependency table for aarch64.
- `[profile.release]` exactly as Decision 9.

`scryd-fetch-weights` helper contract:

- Single argument: `--target <dir>` (default `$XDG_DATA_HOME/scryd/assets/`).
- Behavior: if `target/xtr-weights.gguf` exists with the expected SHA-256, exit 0. Else download from the pinned Hugging Face URL via `reqwest+rustls`, write to `target/xtr-weights.gguf.tmp` mode `0600`, verify SHA-256, rename. On hash mismatch: delete the tmp, exit 1.
- Built and packaged from this slice; runs as the operator, never as root.

`install.sh` contract (intent):

- `set -euo pipefail`.
- Detects `$HOME`, `$XDG_CONFIG_HOME` (default `$HOME/.config`), `$XDG_DATA_HOME` (default `$HOME/.local/share`).
- Refuses to run as root: `[[ $EUID == 0 ]] && { echo "run as your own user, not root"; exit 1; }`.
- Copies `scryd` and `scryd-fetch-weights` to `~/.local/bin/`, mode `0755`.
- Copies `scryd.service` to `~/.config/systemd/user/scryd.service`.
- Runs `~/.local/bin/scryd-fetch-weights --target "$XDG_DATA_HOME/scryd/assets/"`.
- Runs `systemctl --user daemon-reload`.
- Prints the post-install hint block.

CI release workflow shape (intent):

```
on: { push: { tags: ['v*'] } }
jobs:
  x86_64: ... build + package + upload
  aarch64: ... build + package + upload
```

## §5 Sequence

1. **Developer builds locally.** From repo root: `cargo build --release` — workspace builds all member crates and the `scryd` binary using the host's default target. Default features pick `x86_64` Witchcraft features when on x86_64 Linux. `cargo run --release -- serve` runs the daemon against the developer's own XDG dirs.
2. **Tagged release.** Maintainer pushes a `vX.Y.Z` tag → GitHub Actions runs the matrix → each job builds the workspace, packages the per-arch tarball, attaches it to the release. Witchcraft is fetched at build time from the pinned commit hash.
3. **Operator install.** Operator downloads `scryd-vX.Y.Z-x86_64-linux.tar.gz`, extracts, runs `./install.sh` → binary lands in `~/.local/bin/`, unit lands in `~/.config/systemd/user/`, weights download to `$XDG_DATA_HOME/scryd/assets/xtr-weights.gguf` (verified SHA-256). Operator runs `scryd add-account`, then `systemctl --user enable --now scryd`. (Optionally: `loginctl enable-linger $USER` to survive logout.)
4. **Daemon start.** systemd user manager spawns `~/.local/bin/scryd serve` → daemon runs multi-instance-isolation startup self-checks → opens `meta.sqlite`, runs migrations → mounts api routes on the UDS → starts imap-sync scheduler → starts indexer task. Witchcraft is constructed with the asset dir from Decision 4.
5. **Operator upgrade.** Maintainer ships `vX.Y+1`. Operator downloads new tarball, runs `./install.sh` → script overwrites the binary, leaves config and data and weights in place, runs `systemctl --user restart scryd`. If the new release pinned a different weight SHA, `scryd-fetch-weights` notices the mismatch and re-downloads. Storage migrations apply forward at next start.
6. **Operator uninstall.** No script ships for this in v1; documented as: `systemctl --user disable --now scryd; rm ~/.local/bin/scryd ~/.config/systemd/user/scryd.service; rm -rf ~/.config/scryd ~/.local/share/scryd`. Rationale: operators can read.

## §6 Out of scope

- Anything inside the application crates' code (every other slice).
- macOS, Windows, BSD builds. Spec is Linux-only.
- musl-static builds; v1 is glibc.
- Snap / Flatpak / Docker images. Tarball only in v1.
- A package that runs as a system-wide daemon under `root:scrye`. The spec was rewritten to per-user-only.
- Auto-update / built-in updater. Operator pulls a new tarball when they want.
- Code signing / Sigstore signatures on the artifact. v2.
- Tab-completion shipping (clap can generate; out for v1 per the cli slice).
- Weight pre-bundling for an air-gapped install. v2; operators with no internet today have to copy the GGUF file in by hand.

## §7 Open questions

- Whether `MemoryDenyWriteExecute=yes` can be enabled in the systemd unit. Requires testing against Witchcraft's `fbgemm` + `t5-quantized` runtime: any JIT/PROT_EXEC that hits a writable mapping will trip it. v1 leaves it off; first task in this slice tries it under load and either commits it or files an explicit follow-up.
- Whether `aarch64-unknown-linux-gnu` builds should additionally enable a Witchcraft NEON feature if one exists upstream (the README does not list one as of recon). Re-recon at code time.
- Whether the install script should detect and warn about glibc < 2.34 (where some `tokio`/`rustls` syscalls behave differently). v1: assume Ubuntu 22.04+ glibc; document the floor in `README.install.md`.
- Whether to fetch weights from Hugging Face directly or stage them on a release-time mirror in a GitHub Release asset. Direct HF requires HF cooperation if the URL changes. v1 fetches from HF; flip to a mirror only if HF rate-limits or removes the artifact.
- Specific dropbox/witchcraft commit hash to pin: settled at first-task time; flagged for refresh in CHANGELOG when bumped.
- Whether `scryd-fetch-weights` should sit alongside `scryd` in `~/.local/bin/` (operator-visible) or be invoked only by `install.sh` and tucked into a private spot. v1: ship in `~/.local/bin/` so the operator can re-run it manually if a release bumps the weights.

> If the parent spec is ambiguous on anything this slice depends on, stop and update the spec. Do not invent behavior here.
