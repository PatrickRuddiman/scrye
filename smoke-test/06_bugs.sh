#!/usr/bin/env bash
# Turn findings.jsonl into GitHub issues. Deduplicates against open
# issues under SCRYD_SMOKE_GH_LABEL with an identical title — re-running
# after a partial fix only files net-new findings.

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "$SCRIPT_DIR/lib.sh"

resolve_gh || die "could not locate gh (install via 'scoop install gh' or 'apt install gh')"
need_cmd python3

[[ -f "$SMOKE_FINDINGS" && -s "$SMOKE_FINDINGS" ]] || { log "no findings to file"; exit 0; }

if ! "$SMOKE_GH" auth status >/dev/null 2>&1; then
    die "gh not authenticated — run 'gh auth login'"
fi

# Ensure the dedupe label exists on the target repo. `gh label create`
# is non-idempotent (errors on duplicate) so check first.
if ! "$SMOKE_GH" label list --repo "$SCRYD_SMOKE_GH_REPO" --json name -q '.[].name' \
        | grep -qx "$SCRYD_SMOKE_GH_LABEL"; then
    log "creating label '$SCRYD_SMOKE_GH_LABEL' on $SCRYD_SMOKE_GH_REPO"
    "$SMOKE_GH" label create "$SCRYD_SMOKE_GH_LABEL" \
        --repo "$SCRYD_SMOKE_GH_REPO" \
        --color "B60205" \
        --description "Filed automatically by smoke-test/" \
        >/dev/null
fi

# Snapshot open issue titles once per run to avoid N+1 lookups.
mapfile -t EXISTING < <("$SMOKE_GH" issue list \
    --repo "$SCRYD_SMOKE_GH_REPO" \
    --label "$SCRYD_SMOKE_GH_LABEL" \
    --state open \
    --limit 200 \
    --json title -q '.[].title')

issue_exists() {
    local needle="$1"
    local t
    for t in "${EXISTING[@]:-}"; do
        [[ "$t" == "$needle" ]] && return 0
    done
    return 1
}

CREATED=0
SKIPPED=0
total="$(wc -l < "$SMOKE_FINDINGS" | tr -d ' ')"
log "filing $total finding(s) against $SCRYD_SMOKE_GH_REPO"

while IFS= read -r line; do
    [[ -z "$line" ]] && continue
    title="$(printf '%s' "$line" | python3 -c 'import json,sys; print(json.loads(sys.stdin.read())["title"])')"
    check="$(printf '%s' "$line" | python3 -c 'import json,sys; print(json.loads(sys.stdin.read())["check"])')"
    summary="$(printf '%s' "$line" | python3 -c 'import json,sys; print(json.loads(sys.stdin.read())["summary"])')"
    ts="$(printf '%s' "$line" | python3 -c 'import json,sys; print(json.loads(sys.stdin.read())["ts"])')"
    log_path="$(printf '%s' "$line" | python3 -c 'import json,sys; print(json.loads(sys.stdin.read()).get("log") or "")')"

    if issue_exists "$title"; then
        SKIPPED=$((SKIPPED+1))
        log "  skip (already open): $title"
        continue
    fi

    body="$(printf '**Filed by:** \`smoke-test/06_bugs.sh\`\n**Check id:** \`%s\`\n**Detected at:** %s UTC\n**VM:** \`%s@%s\` (rg=\`%s\`)\n\n## Summary\n\n%s\n' \
        "$check" "$ts" "${SMOKE_VM_USER:-?}" "${SMOKE_VM_IP:-?}" "$SCRYD_SMOKE_RG" "$summary")"

    if [[ -n "$log_path" && -f "$log_path" ]]; then
        body+=$'\n\n## Full log\n\n```\n'
        body+="$(tail -n 200 "$log_path")"
        body+=$'\n```\n'
    fi

    body+=$'\n\n## Repro\n\n```sh\ncd smoke-test && ./run.sh\n```\n'

    log "  create: $title"
    "$SMOKE_GH" issue create \
        --repo "$SCRYD_SMOKE_GH_REPO" \
        --label "$SCRYD_SMOKE_GH_LABEL" \
        --title "$title" \
        --body "$body" </dev/null >/dev/null
    CREATED=$((CREATED+1))
done < "$SMOKE_FINDINGS"

log "filed $CREATED issue(s); skipped $SKIPPED already-open"
