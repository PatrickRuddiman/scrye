Parent slice: [scryd v0.2.0 — build-and-packaging](../../slices/0.2.0/build-and-packaging.md)
Depends on: 04, 05

# Task 06 — release-yml-linux-only

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Drop the macOS jobs from `.github/workflows/release.yml`, ship `uninstall.sh` + the new `.in` templates in the v0.2.0 tarballs, remove the now-orphan `ops/scryd.service` file from the repo, and keep the existing license-check + sha256 + GitHub-release-create flow.

## Tasks
- [x] In `.github/workflows/release.yml`, remove the matrix entries for `target: x86_64-apple-darwin` (on `macos-13`) and `target: aarch64-apple-darwin` (on `macos-14`). Keep only `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu`.
- [x] Remove the per-step `if: matrix.os_name == 'linux'` and `if: matrix.os_name == 'macos'` conditionals — every remaining job is Linux. Drop the `os_name` and `cross` matrix fields where they only existed to gate macOS or to switch between native and `cross` (keep `cross` only for the aarch64 row).
- [x] In the package step, update the file list copied into `$stage/$out/` to include: `target/${{ matrix.target }}/release/scryd`, `target/${{ matrix.target }}/release/scryd-fetch-weights`, `LICENSE`, `ops/scryd.service.in`, `ops/scryd.tmpfiles.in`, `ops/install.sh`, `ops/uninstall.sh`, `ops/README.install.md`. Remove the `ops/scryd.service` copy.
- [x] Update the tarball naming to `scryd-${tag}-${arch}-linux.tar.gz` (drop the `${os_name}` interpolation since everything is Linux).
- [x] Delete `ops/scryd.service` from the repo. The file is replaced by `ops/scryd.service.in` rendered at install time.
- [x] Verify YAML still parses with `python -c 'import yaml; yaml.safe_load(open(".github/workflows/release.yml"))'`.

## Acceptance criteria
- [x] `test -f .github/workflows/release.yml`.
- [x] `python -c 'import yaml; yaml.safe_load(open(".github/workflows/release.yml"))'` exits 0.
- [x] `! grep -F 'apple-darwin' .github/workflows/release.yml` (no macOS targets).
- [x] `! grep -F 'macos-13' .github/workflows/release.yml`.
- [x] `! grep -F 'macos-14' .github/workflows/release.yml`.
- [x] `grep -F 'x86_64-unknown-linux-gnu' .github/workflows/release.yml` matches.
- [x] `grep -F 'aarch64-unknown-linux-gnu' .github/workflows/release.yml` matches.
- [x] `grep -F 'ops/uninstall.sh' .github/workflows/release.yml` matches.
- [x] `grep -F 'ops/scryd.service.in' .github/workflows/release.yml` matches.
- [x] `grep -F 'ops/scryd.tmpfiles.in' .github/workflows/release.yml` matches.
- [x] `! grep -F 'ops/scryd.service' .github/workflows/release.yml` (the non-template path is gone; only the `.in` paths remain).
- [x] `! test -f ops/scryd.service` (the orphan file is deleted).

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
