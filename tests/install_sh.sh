#!/usr/bin/env bash
# Smoke test for ops/install.sh.
#
# Builds a fixture bundle dir alongside ops/install.sh containing stub
# scryd / scryd-fetch-weights / scryd.service files, runs install.sh
# against a temp HOME, and asserts the resulting per-user filesystem
# layout matches the documented contract.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
INSTALL_SH="$REPO_ROOT/ops/install.sh"

if [[ ! -x "$INSTALL_SH" ]]; then
    echo "FAIL: $INSTALL_SH is not executable" >&2
    exit 1
fi

# Build a fixture bundle with stubbed binaries.
BUNDLE="$(mktemp -d)"
trap 'rm -rf "$BUNDLE" "$FAKE_HOME"' EXIT

cp "$INSTALL_SH" "$BUNDLE/install.sh"
chmod +x "$BUNDLE/install.sh"

cat > "$BUNDLE/scryd" <<'STUB'
#!/usr/bin/env bash
echo "fake-scryd $@"
STUB
chmod +x "$BUNDLE/scryd"

cat > "$BUNDLE/scryd-fetch-weights" <<'STUB'
#!/usr/bin/env bash
echo "fake-fetch-weights $@"
STUB
chmod +x "$BUNDLE/scryd-fetch-weights"

cp "$REPO_ROOT/ops/scryd.service" "$BUNDLE/scryd.service"

# Run install.sh against a temp HOME with weights download + systemctl
# both skipped (no systemd in the test container).
FAKE_HOME="$(mktemp -d)"
SCRYD_INSTALL_ALLOW_ROOT=1 \
SCRYD_INSTALL_SKIP_SYSTEMCTL=1 \
SCRYD_INSTALL_SKIP_WEIGHTS=1 \
HOME="$FAKE_HOME" \
    bash "$BUNDLE/install.sh"

# Assert the layout.
test_paths=(
    "$FAKE_HOME/.local/bin/scryd"
    "$FAKE_HOME/.local/bin/scryd-fetch-weights"
    "$FAKE_HOME/.config/systemd/user/scryd.service"
    "$FAKE_HOME/.config/scryd"
    "$FAKE_HOME/.local/share/scryd"
    "$FAKE_HOME/.local/share/scryd/assets"
)
for p in "${test_paths[@]}"; do
    if [[ ! -e "$p" ]]; then
        echo "FAIL: $p missing after install" >&2
        exit 1
    fi
done

# Assert dir modes.
for d in \
    "$FAKE_HOME/.config/scryd" \
    "$FAKE_HOME/.local/share/scryd" \
    "$FAKE_HOME/.local/share/scryd/assets"
do
    mode=$(stat -c '%a' "$d")
    if [[ "$mode" != "700" ]]; then
        echo "FAIL: $d mode $mode != 700" >&2
        exit 1
    fi
done

# Assert binary modes.
for f in "$FAKE_HOME/.local/bin/scryd" "$FAKE_HOME/.local/bin/scryd-fetch-weights"; do
    mode=$(stat -c '%a' "$f")
    if [[ "$mode" != "755" ]]; then
        echo "FAIL: $f mode $mode != 755" >&2
        exit 1
    fi
done

# Stub binary content survived the copy.
if ! grep -q "fake-scryd" "$FAKE_HOME/.local/bin/scryd"; then
    echo "FAIL: scryd content was not copied" >&2
    exit 1
fi

echo "OK: install.sh smoke test passed"
