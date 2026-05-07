# scripts

Helpers for working on scryd from a non-Linux host.

## `dev.ps1` / `dev.sh`

Spawns a one-shot or interactive Debian-based Rust container with the repo
mounted at `/workspace`. Linux build artifacts go into a named Docker volume
(`mail-clawd-target`) instead of the host's `target/` directory, so cross-
platform development doesn't clobber the host's build cache. The cargo
registry caches in another named volume (`mail-clawd-cargo`) so dependency
fetches survive container restarts.

### Windows (PowerShell)

```powershell
.\scripts\dev.ps1                          # interactive bash shell
.\scripts\dev.ps1 cargo check --workspace
.\scripts\dev.ps1 cargo test -p scryd-config
.\scripts\dev.ps1 cargo build --release -p scryd
```

### Linux / macOS / WSL

```bash
./scripts/dev.sh                           # interactive bash shell
./scripts/dev.sh cargo check --workspace
./scripts/dev.sh cargo test -p scryd-config
```

### Image

`rust:1-bookworm` (official Rust image on Debian 12). Has rustc, cargo, gcc,
and build-essential. Tasks that need extra system packages (e.g., cmake,
clang for the Witchcraft build in task 09) install them on demand inside the
container or via a future `Dockerfile` shipped alongside this script.

### First run

On first run, Docker pulls `rust:1-bookworm` (~700 MB compressed). Subsequent
runs are instant. The named volumes are created automatically.

### Reset

```powershell
docker volume rm mail-clawd-target mail-clawd-cargo
```

removes the cached target dir and cargo registry without touching the repo.

### PowerShell 5.1 cosmetic note

Windows PowerShell 5.1 (the default `powershell.exe`) wraps every line a
native command writes to stderr into an `ErrorRecord` and renders it red,
even when the underlying process exits `0`. `cargo` and `rustup` write
progress messages to stderr by design. The wrapper still propagates the
correct exit code via `exit $LASTEXITCODE`. PowerShell 7 (`pwsh`) does not
have this quirk; if the red noise bothers you, invoke the script with
`pwsh -File .\scripts\dev.ps1 …`.
