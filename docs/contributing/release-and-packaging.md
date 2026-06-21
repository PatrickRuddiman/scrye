---
sources:
  - .github/workflows/release.yml
  - .github/workflows/ci.yml
  - .github/workflows/witchcraft-assets.yml
  - .github/actions/build-linux-packages/action.yml
  - scripts/build-packages.sh
  - ops/nfpm/nfpm.yaml
  - ops/nfpm/scripts/postinstall.sh
  - ops/nfpm/scripts/preremove.sh
  - ops/nfpm/scripts/postremove.sh
  - ops/README.release.md
---

# Release and packaging

This page describes how release artifacts are built and published. For how to
install them, see [Installation](../installation.md).

## Release workflow

The `release` workflow triggers on a pushed `v*` tag (or manually). It runs three
stages:

1. **License check** with `cargo-deny`.
2. **Build matrix** across `x86_64-unknown-linux-gnu` (native) and
   `aarch64-unknown-linux-gnu` (built with `cross`).
3. **GitHub release**, on a tag only.

Each build job verifies the systemd unit, stages model assets and runs the test
and end-to-end suites on the native target, builds the release binaries
(`scryd` and `scryd-fetch-weights`), assembles a tarball, and builds the native
packages. The release job aggregates every artifact, writes a `SHA256SUMS`
manifest, and creates the GitHub release with generated notes.

## Tarball

The tarball is named `scryd-<tag>-<arch>-linux.tar.gz` (with a matching `.sha256`)
and bundles the two binaries, `LICENSE`, the systemd unit template, `install.sh`,
`uninstall.sh`, and the install README. The installer places binaries in
`/usr/local/bin`. See [Installation](../installation.md).

## Native packages

Packages are built by the `build-linux-packages` composite action, which installs
nfpm (default version 2.46.3) and runs `scripts/build-packages.sh`. One nfpm
config (`ops/nfpm/nfpm.yaml`) produces four formats from the same inputs: `deb`,
`rpm`, `apk`, and `archlinux`.

The build script stages the release binaries, renders the systemd unit for the
packaged binary path (rewriting `/usr/local/bin` to `/usr/bin`, which also
retargets the weight-fetch helper override), and runs nfpm once per format. The
package version is the pushed `v*` tag with its leading `v` stripped, or the
workspace `Cargo.toml` version when no tag is set.

Unlike the tarball, packages install binaries to `/usr/bin` and the unit to
`/usr/lib/systemd/system`. Their maintainer scripts create the `scryd` user, lay
out the FHS tree, and — on removal — preserve `/etc/scryd` and `/var/lib/scryd`.
See [Upgrades and removal](../operations/upgrades.md).

## Model asset bundle

The witchcraft model bundle is produced separately by the `witchcraft-assets`
workflow against a pinned `dropbox/witchcraft` revision and published as a
`witchcraft-assets-<rev>` release. Both CI and the `scryd-fetch-weights` default
URL point at that pinned release. See
[Weights and assets](../operations/weights-and-assets.md).

## See also

- [Installation](../installation.md)
- [Upgrades and removal](../operations/upgrades.md)
- [Testing](testing.md)
- [Weights and assets](../operations/weights-and-assets.md)
