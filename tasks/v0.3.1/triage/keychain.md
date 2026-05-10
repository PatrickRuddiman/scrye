# Triage: OS keychain credential storage

**Status:** Downgraded under v0.3.1. Service deploys lean on infrastructure secret managers (Vault, AWS Secrets Manager, k8s Secrets) rather than per-host OS keychains.

**Why downgraded:** v0.3.1 dropped the per-operator-host shape, so the original "operator's gnome-keyring stores the credential" UX no longer fits. The dedicated `scryd` system user has no session keychain to talk to.

**Triggers:** A deploy that wants to keep `/etc/scryd/config.toml` from holding the credential at all. A small helper that reads from the OS keychain at startup and pipes the secret to the daemon over a private fd would work.

**Sketch:** `[[accounts]] password_file = "/run/scryd/credentials.d/<id>.cred"` field; an external helper writes the file from whatever secret manager the operator runs; daemon reads at LOGIN time.
