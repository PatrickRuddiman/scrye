---
sources:
  - crates/scryd-fetch-weights/src/main.rs
  - crates/scryd-runtime/src/serve.rs
  - ops/install.sh
  - .github/workflows/witchcraft-assets.yml
---

# Weights and assets

Semantic search needs the witchcraft model bundle on disk. This page covers what
those assets are, how they are fetched, and how to stage them offline.

## What the assets are

witchcraft's `t5-quantized` backend loads three files at runtime, kept in the
`assets/` directory under the data dir (`/var/lib/scryd/assets` under the systemd
unit):

- `config.json`
- `tokenizer.json`
- `xtr.gguf`

They are distributed as a single tarball published as a GitHub release on this
repository, produced by the `witchcraft-assets` workflow against a pinned
`dropbox/witchcraft` revision.

## The fetch helper

`scryd-fetch-weights` downloads the bundle, verifies its SHA-256, and extracts the
three files. Re-running it when all three are already present is a no-op.

| Flag | Default |
| --- | --- |
| `--target <dir>` | `$XDG_DATA_HOME/scryd/assets`, else `$HOME/.local/share/scryd/assets`. |
| `--url <url>` | The pinned release tarball URL. |
| `--sha256 <hex>` | The pinned bundle hash (see resolution below). |

The expected hash is resolved in order: the `--sha256` flag, the
`SCRYD_PINNED_WEIGHTS_SHA256` environment variable, a build-time `WEIGHTS_SHA256`,
then the compile-time default. A download whose hash does not match is deleted and
the run fails. Extraction shells out to `tar`, which must be on `PATH`.

## First-start auto-fetch

On first start, if any of the three files is missing, the daemon runs the helper
itself with `--target <assets>` before opening the index. The helper path defaults
to `/usr/local/bin/scryd-fetch-weights` and is overridable with
`SCRYD_FETCH_WEIGHTS_BIN`; the native packages set that variable to
`/usr/bin/scryd-fetch-weights` in the unit, matching where they install the
helper. See [Daemon lifecycle](../concepts/lifecycle.md).

The tarball installer pre-fetches the bundle during install (and widens the three
files to mode `0644` so search-time reads succeed), so a tarball install usually
has the assets in place before the first start.

## Staging offline

On a host with no internet access, put the assets in place before starting the
daemon so the auto-fetch becomes a no-op. Either:

- copy a known-good `config.json`, `tokenizer.json`, and `xtr.gguf` into the
  `assets/` directory directly, or
- run the helper against a reachable mirror, pinning the hash:

```
scryd-fetch-weights --target /var/lib/scryd/assets \
  --url https://your-mirror/xtr-gguf.tar.gz \
  --sha256 <expected-hex>
```

Make sure the files are readable by the `scryd` user.

## See also

- [Installation](../installation.md)
- [Daemon lifecycle](../concepts/lifecycle.md)
- [Indexing and search](../concepts/indexing-and-search.md)
- [Troubleshooting](troubleshooting.md)
