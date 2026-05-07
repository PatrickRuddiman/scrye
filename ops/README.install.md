# Installing scryd

scryd is a per-user daemon. Each operator on a Linux host installs and
runs their own instance under their own user account; OS-level identity
prevents one operator's processes from reaching another's instance.

## Quick install

Download the per-arch tarball from GitHub Releases for your host arch
(`x86_64-linux` or `aarch64-linux`), extract, and run the bundled
installer:

```sh
tar xzf scryd-vX.Y.Z-x86_64-linux.tar.gz
cd scryd-vX.Y.Z-x86_64-linux
./install.sh
```

`install.sh` does:

1. Refuses to run as root.
2. Resolves `$XDG_CONFIG_HOME` (default `~/.config`) and `$XDG_DATA_HOME`
   (default `~/.local/share`).
3. Copies `scryd` and `scryd-fetch-weights` to `~/.local/bin/` (mode `0755`).
4. Copies `scryd.service` to `~/.config/systemd/user/scryd.service`.
5. Creates `$XDG_CONFIG_HOME/scryd/`, `$XDG_DATA_HOME/scryd/`, and
   `$XDG_DATA_HOME/scryd/assets/` at mode `0700`.
6. Runs `scryd-fetch-weights --target $XDG_DATA_HOME/scryd/assets/` to
   download and SHA-256-verify the T5 weights bundle.
7. Runs `systemctl --user daemon-reload` so the new unit is visible.
8. Prints next steps.

If `~/.local/bin` is not in your `$PATH`, the script warns; add this
to your shell rc:

```sh
export PATH="$HOME/.local/bin:$PATH"
```

## After install

```sh
scryd add-account                    # interactive credential capture
systemctl --user enable --now scryd  # start now + enable on next login

# Optional: survive logout / start at boot
loginctl enable-linger "$USER"
```

## Manual install (no installer)

```sh
mkdir -p ~/.local/bin ~/.config/systemd/user ~/.config/scryd ~/.local/share/scryd/assets
chmod 0700 ~/.config/scryd ~/.local/share/scryd ~/.local/share/scryd/assets

install -m 0755 ./scryd                  ~/.local/bin/scryd
install -m 0755 ./scryd-fetch-weights    ~/.local/bin/scryd-fetch-weights
install -m 0644 ./scryd.service          ~/.config/systemd/user/scryd.service

~/.local/bin/scryd-fetch-weights --target ~/.local/share/scryd/assets
systemctl --user daemon-reload
```

The weights bundle URL and expected SHA-256 are baked into
`scryd-fetch-weights`; for air-gapped installs override with
`--url file:///path/to/local/copy.gguf --sha256 <hex>`.

## Uninstall

scryd doesn't ship an uninstall script; the layout is well-defined:

```sh
systemctl --user disable --now scryd
rm ~/.local/bin/scryd ~/.local/bin/scryd-fetch-weights
rm ~/.config/systemd/user/scryd.service
rm -rf ~/.config/scryd ~/.local/share/scryd
```

This removes the binary, the unit, the config (including the IMAP
credential), the indexed mail cache, and the T5 weights. systemd
flushes the unit's enabled state on `disable`.

## Logs

scryd writes structured JSON-Lines to stderr; systemd captures it into
the per-user journal.

```sh
# tail
journalctl --user -u scryd -f

# all errors for one account
journalctl --user -u scryd --output cat \
  | jq 'select(.level=="error" and .account_id=="primary")'

# failure-category breakdown
journalctl --user -u scryd --output cat -n 200 \
  | jq -r '.category // empty' | sort | uniq -c | sort -rn
```

## Multi-user hosts

Multiple operators can install their own scryd instance on the same
host. Each operator runs `./install.sh` as their own user; the per-user
`$XDG_RUNTIME_DIR` and `$XDG_DATA_HOME` paths give each instance its
own socket, data dir, and weights file. OS-level file permissions
prevent one operator's processes from reaching another operator's
instance — see the spec for the full trust-boundary discussion.

## Floor: glibc 2.34+

scryd targets glibc 2.34 or newer (Ubuntu 22.04, Debian 12, Fedora 36,
RHEL 9). On older distros, build from source against your local
toolchain.
