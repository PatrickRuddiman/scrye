#!/usr/bin/env bash
# Orchestrator: provision -> install -> configure -> exercise.
# Bug filing and teardown are explicit (`./06_bugs.sh`, `./cleanup.sh`)
# so a human can review findings before issues hit GH and so the VM
# stays up for manual poking after a failure.

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

bash "$SCRIPT_DIR/01_keys.sh"
bash "$SCRIPT_DIR/02_vm.sh"
bash "$SCRIPT_DIR/03_install.sh"
bash "$SCRIPT_DIR/04_account.sh"

# 05 returns non-zero on failures; we still want to print the summary
# and let the operator decide whether to file bugs / tear down.
rc=0
bash "$SCRIPT_DIR/05_smoke.sh" || rc=$?

echo
echo "smoke-test summary"
echo "  exit code:    $rc"
echo "  findings:     $SCRIPT_DIR/.state/findings.jsonl"
echo "  logs:         $SCRIPT_DIR/.state/logs/"
echo
echo "next steps:"
echo "  ./06_bugs.sh     # file each finding as a GH issue (dedup by title)"
echo "  ./cleanup.sh     # delete the Azure resource group"

exit "$rc"
