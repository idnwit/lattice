#!/usr/bin/env bash
# Check that commit subjects (or one given subject) follow Conventional Commits.
#
#   scripts/check-conventional.sh --subject "feat(relay): add tokens"
#   scripts/check-conventional.sh --range <base>..<head>
#
# Used by CI (.github/workflows/conventional.yml) and the local commit-msg hook.
set -euo pipefail

TYPES='feat|fix|perf|refactor|docs|test|build|ci|chore|revert'
RE="^(${TYPES})(\([a-z0-9-]+\))?!?: [a-z].*[^.]$"
MAX_LEN=72

check() {
  local subject="$1" where="$2"
  if [[ ! "$subject" =~ $RE ]]; then
    echo "::error::${where}: not a Conventional Commit: \"${subject}\""
    echo "  expected: <type>(<scope>)?!?: <lowercase subject, no trailing period>"
    echo "  types: ${TYPES//|/ }"
    return 1
  fi
  if (( ${#subject} > MAX_LEN )); then
    echo "::error::${where}: subject longer than ${MAX_LEN} chars (${#subject}): \"${subject}\""
    return 1
  fi
}

case "${1:-}" in
  --subject)
    check "$2" "subject"
    ;;
  --range)
    status=0
    if [[ -n "$(git rev-list --merges "$2")" ]]; then
      echo "::error::merge commits are not allowed; rebase onto main instead"
      status=1
    fi
    while IFS=$'\t' read -r sha subject; do
      check "$subject" "commit ${sha:0:8}" || status=1
    done < <(git log --format='%H%x09%s' "$2")
    exit "$status"
    ;;
  *)
    echo "usage: $0 --subject <text> | --range <base>..<head>" >&2
    exit 2
    ;;
esac
