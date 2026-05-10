# Triage: macOS / Windows ports

**Status:** Out of v0.3.x. Linux x86_64 / aarch64 only.

**Why deferred:** Witchcraft's candle-core dependency has Linux-tested binary backends (fbgemm, hybrid-dequant). macOS has metal but the Apple Silicon variant is undertested in this codebase. Windows lacks a Unix-domain-socket abstraction equivalent at our hyper layer + would need a port of the dedicated-system-user model to Windows services.

**Triggers:** A consumer who wants scryd in a Windows-on-Mac dev workflow, or as part of a macOS-native productivity app.

**Sketch:** macOS port is the smaller lift (Unix sockets work; need launchd plist instead of systemd unit). Windows is a separate spec — TCP-only API, scheduled task instead of unit, no UDS.
