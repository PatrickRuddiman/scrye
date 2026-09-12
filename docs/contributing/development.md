---
sources:
  - Cargo.toml
  - rust-toolchain.toml
  - scryd/src/main.rs
  - crates/scryd-search/src/lib.rs
  - crates/scryd-runtime/src/serve.rs
  - crates/scryd-runtime/src/xdg.rs
---

# Development

This page covers building and running scryd from source. For the crate map see
[Repository layout](repo-layout.md); for tests see [Testing](testing.md).

## Toolchain

The workspace pins the stable Rust channel (`rust-toolchain.toml`) and sets a
minimum supported Rust version of 1.78 (`Cargo.toml`). The release profile builds
with fat LTO, a single codegen unit, stripped symbols, and `panic = "abort"`.

## Platform support

The production index engine (witchcraft, via candle and fbgemm) is compiled in on
Unix targets only — Linux and macOS. On other hosts, such as a Windows
development box, the witchcraft binding is absent and the workspace type-checks
against the in-memory index engine instead. Building, testing, and running the
full daemon are Unix activities. See [Indexing and search](../concepts/indexing-and-search.md).

The first build pulls pinned git revisions of `witchcraft` and `candle`, so it
takes longer and needs network access.

## Build

```
cargo build
cargo build --release --target <triple> -p scryd -p scryd-fetch-weights
```

The release invocation mirrors what CI builds for distribution.

## Run locally

scryd resolves its config and data locations from the environment. Point them at
writable scratch paths, set the mailbox to serve, then run the binary:

```
export USER_EMAIL=you@example.com
export XDG_CONFIG_HOME=/tmp/scryd-config   # holds scryd/config.toml
export XDG_DATA_HOME=/tmp/scryd-data        # holds meta.sqlite, witchcraft.sqlite, raw/, assets/
# optional: override the loopback MCP bind
export SCRYD_MCP_BIND=127.0.0.1:7878
cargo run -p scryd
```

Create `$XDG_CONFIG_HOME/scryd/config.toml` with an `[[accounts]]` entry whose
`user` equals `USER_EMAIL`. See [Configuration](../configuration.md) for the
schema and [Path resolution](../configuration.md#file-location) for how these
variables map to paths.

On first start the daemon fetches the model weights if they are absent. To point
it at a specific helper binary, set `SCRYD_FETCH_WEIGHTS_BIN`. See
[Weights and assets](../operations/weights-and-assets.md).

## See also

- [Repository layout](repo-layout.md)
- [Testing](testing.md)
- [Configuration](../configuration.md)
- [Architecture](../concepts/architecture.md)
