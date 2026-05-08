# Releasing scryd

Per build-and-packaging slice §3 Decision 10 + §3 Decision 12: scryd ships
as per-arch tarballs attached to a GitHub Release. There is no Debian /
RPM / Homebrew / AUR / Nix story in v1; downstream packaging is a v2
ergonomic.

The release pipeline lives in `.github/workflows/release.yml`.

## What gets built

For every pushed tag matching `v*`, the matrix builds two release
tarballs:

- `scryd-vX.Y.Z-x86_64-linux.tar.gz` (built natively on `ubuntu-22.04`)
- `scryd-vX.Y.Z-aarch64-linux.tar.gz` (cross-compiled via `cross`)

Each tarball contains:

- `scryd` — the daemon + CLI binary
- `scryd-fetch-weights` — helper that downloads + verifies the T5 GGUF
  weights on first install
- `scryd.service` — the hardened systemd user unit
- `install.sh` — the per-user installer (`./install.sh` from inside the
  extracted directory)
- `README.install.md` — operator-facing install / uninstall / log
  recipes
- `LICENSE` — Apache-2.0

A `<tarball>.sha256` sidecar file is uploaded alongside each tarball so
downstream consumers can verify integrity without needing the GitHub UI.

Weights are **not** in the tarball (slice §3 Decision 4). They live at
`$XDG_DATA_HOME/scryd/assets/xtr-weights.gguf` and are fetched on first
install by `scryd-fetch-weights`.

## Cutting a release

1. **Bump the version** in the workspace `Cargo.toml`'s `[workspace.package]`
   `version` field. Commit (`chore: vX.Y.Z`).
2. **Tag and push:**
   ```sh
   git tag -a vX.Y.Z -m "scryd vX.Y.Z"
   git push origin vX.Y.Z
   ```
3. **Watch the workflow** at
   `https://github.com/PatrickRuddiman/scrye/actions/workflows/release.yml`.
   Both matrix jobs must succeed:
   - `license check (cargo deny)` — every dependency license must be on
     the allow-list in `deny.toml`.
   - `build (x86_64-unknown-linux-gnu)`
   - `build (aarch64-unknown-linux-gnu)`
4. **Verify the release.** When the workflow finishes, the GitHub
   Release page for the tag shows two `.tar.gz` files and two
   `.sha256` sidecars. Download both tarballs and confirm their SHA-256
   matches the sidecar.
5. **Edit the release notes manually** with what changed. The workflow
   auto-generates a commit list (`generate_release_notes: true`); replace
   it with operator-facing prose: behavior changes, weight-pin bumps,
   migration notes.

## Dry runs (no tag, no upload)

The workflow exposes `workflow_dispatch` so a maintainer can trigger a
manual run from the Actions UI. With `dry_run: "true"` (the default), the
upload step is skipped because the trigger has no `refs/tags/v*` ref —
the build + test + package steps still run, and you get the tarballs as
workflow artifacts under "Artifacts" on the run page.

This is the way to validate a release-pipeline change without burning a
real version number.

## Pre-flight check (local)

Before tagging, run the same checks the workflow runs. From the repo
root:

```sh
cargo build --release --workspace
cargo test --workspace
systemd-analyze verify ops/scryd.service   # Linux only
cargo deny check                           # cargo install --locked cargo-deny
```

If all four exit 0, the workflow will too.

## Provenance + supply-chain

- Both binaries are built with `[profile.release]` from the workspace
  `Cargo.toml`: `opt-level = 3`, `lto = "fat"`, `codegen-units = 1`,
  `strip = "symbols"`, `panic = "abort"` (slice §3 Decision 9).
- `cargo deny check` enforces the license allow-list (`deny.toml`) and
  fails the run on a yanked-crate advisory.
- The GGUF weights download URL + SHA-256 are baked into
  `scryd-fetch-weights`'s source. Bump them in the same commit that
  bumps Witchcraft's pinned revision so weights and code stay in sync.

## Out of scope (v1)

- Code signing or Sigstore signatures on tarballs (slice §6).
- Auto-update / built-in updater. Operators pull a new tarball and
  re-run `./install.sh`.
- musl-static or non-glibc Linux targets (slice §6).
- macOS / Windows / BSD targets. scryd is Linux-only by spec.
