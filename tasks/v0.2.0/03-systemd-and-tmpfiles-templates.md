Parent slice: [scryd v0.2.0 — build-and-packaging](../../slices/0.2.0/build-and-packaging.md)
Depends on: 01

# Task 03 — systemd-and-tmpfiles-templates

_Tick `[x]` on each Tasks item as you finish it, and on each Acceptance item as it passes. The unticked state is what tells the next planning run that this task is still safe to edit in place._

## Goal
Ship the systemd system unit + tmpfiles drop-in as install-time templates (`scryd.service.in`, `scryd.tmpfiles.in`) substituting `__UID__` for `SCRYD_ALLOWED_UID` and `__USER__` for the calling Linux user. The renderer (install.sh) lives in task 04; this task only produces the `.in` files.

## Tasks
- [ ] Create `ops/scryd.service.in`. Contents (per build-and-packaging slice §4):
  - `[Unit]` block: `Description=scryd — read-only IMAP indexer & search daemon`, `After=network-online.target`, `Wants=network-online.target`.
  - `[Service]` block: `Type=simple`, `User=scryd`, `Group=scryd`, `ExecStart=/usr/local/bin/scryd serve`, `Restart=on-failure`, `RestartSec=5`, `Environment=SCRYD_ALLOWED_UID=__UID__`, `Environment=XDG_CONFIG_HOME=/etc/scryd`, `Environment=XDG_DATA_HOME=/var/lib/scryd`, `Environment=XDG_RUNTIME_DIR=/run/scryd`, `NoNewPrivileges=yes`, `ProtectSystem=strict`, `ProtectHome=true`, `ReadWritePaths=/etc/scryd /var/lib/scryd /run/scryd`, `PrivateTmp=yes`, `PrivateDevices=yes`, `LockPersonality=yes`, `RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX`, `SystemCallFilter=@system-service`, `SystemCallArchitectures=native`.
  - `[Install]` block: `WantedBy=multi-user.target`.
- [ ] Create `ops/scryd.tmpfiles.in`. Single-line content: `d /run/scryd 0750 scryd __USER__ -`.
- [ ] Verify `__UID__` and `__USER__` are the only substitution placeholders. install.sh in task 04 uses `sed -e 's/__UID__/...' -e 's/__USER__/...'` — any literal `__` sequence elsewhere in the templates would be a substitution hazard.
- [ ] Do NOT delete `ops/scryd.service` yet. Task 06 removes it together with the release.yml change so the v0.1.0 install path keeps working until the release pipeline updates simultaneously.

## Acceptance criteria
- [ ] `test -f ops/scryd.service.in`.
- [ ] `test -f ops/scryd.tmpfiles.in`.
- [ ] `grep -E '^User=scryd$' ops/scryd.service.in` matches.
- [ ] `grep -E '^Environment=SCRYD_ALLOWED_UID=__UID__$' ops/scryd.service.in` matches.
- [ ] `grep -E '^Environment=XDG_CONFIG_HOME=/etc/scryd$' ops/scryd.service.in` matches.
- [ ] `grep -E '^Environment=XDG_DATA_HOME=/var/lib/scryd$' ops/scryd.service.in` matches.
- [ ] `grep -E '^Environment=XDG_RUNTIME_DIR=/run/scryd$' ops/scryd.service.in` matches.
- [ ] `grep -E '^WantedBy=multi-user.target$' ops/scryd.service.in` matches.
- [ ] `grep -E '^ProtectHome=true$' ops/scryd.service.in` matches.
- [ ] `grep -F 'd /run/scryd 0750 scryd __USER__ -' ops/scryd.tmpfiles.in` matches.
- [ ] `bash -c 'sed -e "s/__UID__/1000/g" -e "s/__USER__/alice/g" ops/scryd.service.in | grep -E "^Environment=SCRYD_ALLOWED_UID=1000$"'` exits 0 (substitution test).
- [ ] `bash -c 'sed -e "s/__USER__/alice/g" ops/scryd.tmpfiles.in | grep -F "scryd alice"'` exits 0.

> If a `## Tasks` checkbox can't be completed without changing what the parent slice specifies, stop and update the slice. Do not redesign here.
