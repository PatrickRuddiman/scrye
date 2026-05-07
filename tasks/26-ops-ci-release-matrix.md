Parent slice: [build-and-packaging](../slices/build-and-packaging.md)
Depends on: 25

# Task 26 — ops-ci-release-matrix

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Wire the GitHub Actions release workflow that builds per-arch tarballs (x86_64-linux, aarch64-linux), packages them with the binary + helper + systemd unit + install script, and uploads them to GitHub Releases on every `v*` tag.

## Tasks
- [ ] Create `.github/workflows/release.yml`. Trigger: `on: push: { tags: ['v*'] }`. Add a manual `workflow_dispatch` trigger for dry runs.
- [ ] Define a matrix job `build` over `target: [x86_64-unknown-linux-gnu, aarch64-unknown-linux-gnu]`. Set `runs-on: ubuntu-22.04` for both. For aarch64, use `cross-rs/cross` action (or call `cross build --release --target aarch64-unknown-linux-gnu` directly) so the cross toolchain is provided.
- [ ] In each matrix job, run: `cargo build --release --target ${{ matrix.target }} -p scryd -p scryd-fetch-weights`. Witchcraft features are picked automatically via the target-conditional dependency tables wired in Task 09.
- [ ] After build, package the tarball: create a temp dir; copy `target/${{ matrix.target }}/release/scryd`, `target/${{ matrix.target }}/release/scryd-fetch-weights`, `ops/scryd.service`, `ops/install.sh`, `ops/README.install.md`, `LICENSE` into it; tar+gzip into `scryd-${{ github.ref_name }}-<arch>-linux.tar.gz` (where `<arch>` is `x86_64` or `aarch64`).
- [ ] Compute SHA-256 of each tarball; emit it as a workflow output and write it to a `<arch>.sha256` sidecar file in the artifact dir.
- [ ] Upload the tarball + sidecar via `softprops/action-gh-release` (or `actions/upload-release-asset`) to the GitHub Release for the pushed tag.
- [ ] Add a `verify-unit` step that runs `systemd-analyze verify ops/scryd.service` (Ubuntu 22.04 has it preinstalled). If the unit is malformed, the workflow fails before uploading.
- [ ] Add a `cargo test --workspace --target ${{ matrix.target }}` step that runs after build but before package. Tests that require weights are gated `#[ignore]` and skipped in CI.
- [ ] Create a top-level `LICENSE` file at the repo root (Apache-2.0 to match `[workspace.package].license = "Apache-2.0"` from Task 00). The CI's package step expects this file.
- [ ] Write a small `cargo deny` config or equivalent license-allowlist check in CI, so we know every dep license is compatible with our Apache-2.0 ship. Add `cargo deny check` as a workflow step (use `EmbarkStudios/cargo-deny-action`).
- [ ] Document the release process at `ops/README.release.md`: tag `vX.Y.Z`, push, watch the workflow, verify both artifacts upload, edit release notes manually.

## Acceptance criteria
- [ ] `test -f .github/workflows/release.yml`.
- [ ] `grep -E 'x86_64-unknown-linux-gnu' .github/workflows/release.yml` matches.
- [ ] `grep -E 'aarch64-unknown-linux-gnu' .github/workflows/release.yml` matches.
- [ ] `grep -E 'systemd-analyze verify ops/scryd\.service' .github/workflows/release.yml` matches.
- [ ] `grep -E 'cargo deny check' .github/workflows/release.yml` matches.
- [ ] `test -f LICENSE` and `grep -E 'Apache License, Version 2\.0' LICENSE` matches.
- [ ] `test -f ops/README.release.md`.
- [ ] (If the host has `actionlint` available) `actionlint .github/workflows/release.yml` exits 0; otherwise this AC is satisfied by `python -c "import yaml; yaml.safe_load(open('.github/workflows/release.yml'))"` exiting 0.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
