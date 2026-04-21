#!/usr/bin/env bash

set -euo pipefail

usage() {
    cat >&2 <<'EOF'
Usage:
  ./scripts/check_dco_signoff.sh --commit-msg-file <path>
EOF
    exit 1
}

check_commit_msg_file() {
    local commit_msg_file="$1"

    if [[ ! -f "$commit_msg_file" ]]; then
        echo "Commit message file not found: $commit_msg_file" >&2
        exit 1
    fi

    if ! grep -Eq '^Signed-off-by: .+ <[^<>[:space:]]+>$' "$commit_msg_file"; then
        echo "Commit message is missing a DCO sign-off." >&2
        echo "Add one with: git commit --amend -s --no-edit" >&2
        exit 1
    fi
}

case "${1:-}" in
    --commit-msg-file)
        [[ $# -eq 2 ]] || usage
        check_commit_msg_file "$2"
        ;;
    *)
        usage
        ;;
esac
