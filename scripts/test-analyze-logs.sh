#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
ANALYZE="$SCRIPT_DIR/analyze-logs.sh"

if ! command -v jq >/dev/null 2>&1; then
  printf 'jq required\n' >&2
  exit 1
fi

strip_ansi() {
  sed -E $'s/\x1B\[[0-9;]*[[:alpha:]]//g'
}

assert_contains() {
  local haystack="$1"
  local needle="$2"
  if [[ "$haystack" != *"$needle"* ]]; then
    printf "assert_contains failed: missing '%s'\n" "$needle" >&2
    printf '%s\n' '--- output ---' >&2
    printf '%s\n' "$haystack" >&2
    exit 1
  fi
}

log_file=$(mktemp)
trap 'rm -f "$log_file"' EXIT

cat >"$log_file" <<'EOF'
{"ts":"2026-04-12T17:00:00Z","lvl":"info","msg":"menu_loaded","trace_id":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","span_id":"1111111111111111","span":"menu","dur_ms":42,"project":"demo","service":"api","data":{"count":5,"source":"disk"}}
{"ts":"2026-04-12T17:00:01Z","lvl":"error","msg":"menu_failed","trace_id":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","span_id":"2222222222222222","span":"menu","dur_ms":250,"project":"demo","service":"api","err":{"message":"boom"},"data":{"step":"parse"}}
EOF

errors_out=$(bash "$ANALYZE" errors "$log_file" | strip_ansi)
assert_contains "$errors_out" 'menu_failed'
assert_contains "$errors_out" 'boom'

warnings_out=$(bash "$ANALYZE" warnings "$log_file" | strip_ansi)
assert_contains "$warnings_out" '=== Warnings ==='

spans_out=$(bash "$ANALYZE" spans "$log_file" | strip_ansi)
assert_contains "$spans_out" 'menu_loaded'
assert_contains "$spans_out" 'menu_failed'

slow_out=$(bash "$ANALYZE" slow 200 "$log_file" | strip_ansi)
assert_contains "$slow_out" 'menu_failed'

project_out=$(bash "$ANALYZE" project demo "$log_file" | strip_ansi)
assert_contains "$project_out" 'menu_loaded'
assert_contains "$project_out" 'menu_failed'

stats_out=$(bash "$ANALYZE" stats "$log_file" | strip_ansi)
assert_contains "$stats_out" 'Total events:'
assert_contains "$stats_out" 'Unique traces:'

recent_out=$(bash "$ANALYZE" recent 1 "$log_file" | strip_ansi)
assert_contains "$recent_out" 'menu_failed'

search_out=$(bash "$ANALYZE" search menu "$log_file" | strip_ansi)
assert_contains "$search_out" 'menu_loaded'
assert_contains "$search_out" 'menu_failed'

trace_list_out=$(bash "$ANALYZE" trace "$log_file" | strip_ansi)
assert_contains "$trace_list_out" '=== Recent Traces ==='
assert_contains "$trace_list_out" 'aaaaaaaa'

trace_show_out=$(bash "$ANALYZE" trace bbbbbbbb "$log_file" | strip_ansi)
assert_contains "$trace_show_out" '=== Trace: bbbbbbbb… ==='
assert_contains "$trace_show_out" 'menu_failed'
assert_contains "$trace_show_out" 'Trace Summary'

help_out=$(bash "$ANALYZE" help | strip_ansi)
assert_contains "$help_out" 'Usage:'
assert_contains "$help_out" '[ms] [log-file]'
assert_contains "$help_out" 'TEXT [log-file]'

printf 'yes\n' | bash "$ANALYZE" clear "$log_file" >/tmp/teolog-clear.out
clear_out=$(strip_ansi </tmp/teolog-clear.out)
assert_contains "$clear_out" 'Cleared.'
if [[ -s "$log_file" ]]; then
  printf 'clear test failed: log file not truncated\n' >&2
  exit 1
fi

printf 'analyze-logs.sh tests passed\n'
