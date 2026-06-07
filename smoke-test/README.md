# smoke-test/

A minimal end-to-end smoke bed for scryd, running on a real Azure VM
against a real IMAP account. The same scripts can be re-run unchanged
on every release; failures are captured as JSON-line findings and
filed as GitHub issues (dedup by title) so regressions don't get lost.

## What it does

```
01_keys.sh     →  mint a dedicated RSA-4096 keypair under .state/keys/
02_vm.sh       →  az group + Ubuntu 22.04 VM, NSG narrowed to your egress IP
03_install.sh  →  ship source + cargo build --release on the VM + sudo ./install.sh
04_account.sh  →  scryd add-account (junk@pjly.io by default; password via prompt/env/.state secret)
05_smoke.sh    →  drive scryd through operator smoke checks; failures → .state/findings.jsonl
06_bugs.sh     →  turn each finding into a GH issue (dedup by exact title)
07_pressure.sh →  push/run/collect resumable pressure-test monitors and load probes
cleanup.sh     →  az group delete
```

`run.sh` chains 01→05. 06 and cleanup are explicit so a human can
review findings (and the VM) before anything hits GitHub.

## Prerequisites

On the workstation running these scripts:

| Tool | Reason |
| --- | --- |
| `bash` (Git Bash on Windows is fine) | every script is bash |
| `az` (logged in) | VM provisioning; `az login` first |
| `gh` (logged in, `repo` scope) | bug filing |
| `ssh`, `ssh-keygen`, `scp` | VM access |
| `tar` | shipping source to the VM (pipe through `ssh tar -x`) |
| `python3` | small JSON helpers (no third-party deps) |
| `curl` | NSG narrowing best-effort lookup |

On Azure: an active subscription with `Standard_B4ms` quota in
`SCRYD_SMOKE_LOCATION` (defaults to `eastus2`).

## Configure

```sh
cp smoke-test/config.env.example smoke-test/config.env
```

The IMAP password is **never** committed and never lands on a process
command line. Three ways to supply it, in order of precedence:

1. `SCRYD_SMOKE_IMAP_PASSWORD` already in the env (e.g. CI secret).
2. `smoke-test/.state/secrets.env` — sourced by `lib.sh` on every run.
   Lives under `.state/` (gitignored), chmod'd to `0600` on load.
   Create once for repeatable local runs:

   ```sh
   mkdir -p smoke-test/.state
   printf 'SCRYD_SMOKE_IMAP_PASSWORD=%s\n' 'your-password-here' \
       > smoke-test/.state/secrets.env
   chmod 600 smoke-test/.state/secrets.env
   ```

3. Interactive prompt — `04_account.sh` reads silently if neither of
   the above is set.

The chosen value is piped over SSH into `scryd add-account
--password-stdin`, so it never appears in `ps`/argv on the VM.

## Run / rebuild / tear down

```sh
cd smoke-test
./run.sh                       # provision → install → configure → exercise

# After reviewing .state/findings.jsonl:
./06_bugs.sh                   # file open findings as GH issues

# Rebuild/reinstall current workstation source onto the existing VM:
./03_install.sh
./04_account.sh
./05_smoke.sh

# Pressure/load probes are resumable and collect artifacts under .state/pressure/:
./07_pressure.sh push
./07_pressure.sh load
./07_pressure.sh status
./07_pressure.sh collect .state/pressure/latest

# Tear down Azure infrastructure. Add --wait to block until deletion finishes:
./cleanup.sh --wait
```

Each step is independently runnable and idempotent — re-running
`03_install.sh` after a code change rebuilds and reinstalls on the
existing VM without touching the RG. `cleanup.sh` deletes the Azure
resource group and removes `.state/vm.json`; it intentionally leaves
local keys/logs/artifacts in ignored `.state/` for evidence. Delete
`smoke-test/.state/` manually when you no longer need local evidence.

## State layout

Everything is under `smoke-test/.state/` (gitignored):

```
.state/
├── keys/
│   ├── id_ed25519        # 0600, RSA-4096 key material (legacy filename)
│   └── id_ed25519.pub
├── vm.json               # {"ip": "...", "user": "azureuser"}
├── findings.jsonl        # one JSON object per failed check
└── logs/
    ├── version.log
    ├── status_daemon.log
    ├── account_visible.log
    └── ... (one per check)
```

## Adding a check

Edit `05_smoke.sh`. Two helpers:

```bash
run_check  <id> <title-on-failure> <bash-cmd-string>
run_assert <id> <title-on-failure> <bash-cmd-string> <assertion-evaluated-against-$LOG>
```

- `<id>` becomes the log filename (`.state/logs/<id>.log`) and the
  `check` field in `findings.jsonl`.
- `<title-on-failure>` is the GH issue title. **Keep it stable across
  runs** — `06_bugs.sh` dedupes on exact title under the smoke-test
  label. If you reword a title, you'll get a duplicate issue.
- The command runs over SSH on the VM in a login bash shell, so PATH
  picks up `/usr/local/bin/scryd` and `$HOME/.cargo/env`.

## Common failure modes

- **`02_vm.sh` hangs at "waiting for sshd"**: the VM came up but your
  egress IP changed between provisioning and the SSH probe (e.g. VPN
  toggled). Either fix the NSG manually or delete the RG and re-run.
- **`03_install.sh` OOMs during `cargo build`**: bump `SCRYD_SMOKE_VM_SIZE`
  to `Standard_D4s_v5` or larger. `Standard_B4ms` (16 GiB) is the default
  and fits the candle link with headroom; anything smaller risks OOM.
- **`04_account.sh` says "daemon ready" but `account_visible` fails**:
  the install completed but `scryd-fetch-weights` is still downloading
  the 61 MB asset bundle. Wait 30s and re-run `./05_smoke.sh`.
- **`06_bugs.sh` files a duplicate of an existing issue**: someone
  reworded the title on the GH side, or the existing issue was closed.
  The dedupe only checks open issues — closed ones won't suppress new
  reports.
