---
sources:
  - ops/scryd.service.in
  - ops/install.sh
  - crates/scryd-runtime/src/serve.rs
  - crates/scryd-mcp/src/serve.rs
---

# Running scryd

scryd runs as a systemd service. It has no CLI client: you operate it with
`systemctl` and `journalctl`, and you query it over MCP. The binary itself accepts
only `--help` and `--version`.

## Service control

```
sudo systemctl enable --now scryd    # start now and on boot
sudo systemctl restart scryd         # restart
sudo systemctl stop scryd            # stop
sudo systemctl status scryd          # unit state
```

## Set the mailbox to serve

`USER_EMAIL` is mandatory and is the daemon's only authorization boundary. The
daemon refuses to start until it is set. Provide it as a unit override:

```
sudo systemctl edit scryd
# under [Service]:
#   Environment=USER_EMAIL=you@example.com
```

The matching account must also exist in the config file, with `user` equal to
that address. See [Configuration](../configuration.md).

## Apply configuration changes

scryd reads its config file once, at startup. After editing
`/etc/scryd/config.toml` or changing a unit environment variable, restart the
service to apply it:

```
sudo systemctl restart scryd
```

## Watch what it is doing

Logs go to the journal:

```
journalctl -u scryd -f
```

For point-in-time health — uptime, per-account sync state, index-queue depth, and
the crash-loop supervisor — call the MCP [`status`](../mcp/tools/status.md) tool.
See [Observability](../concepts/observability.md).

## The search endpoint

The MCP server listens on loopback at `127.0.0.1:7878/mcp` by default. Override
the bind with `SCRYD_MCP_BIND`; it must stay on a loopback address, or the daemon
exits. See the [MCP interface](../mcp/README.md).

## See also

- [Configuration](../configuration.md)
- [Daemon lifecycle](../concepts/lifecycle.md)
- [Upgrades](upgrades.md)
- [Troubleshooting](troubleshooting.md)
- [MCP interface](../mcp/README.md)
