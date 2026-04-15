#!/usr/bin/env bash
# teolog — NDJSON wide-event log analysis utilities
# Works with any teolog-format NDJSON log file.
#
# Usage: analyze-logs.sh <command> [command-args...] [log-file]
# If log-file is omitted, auto-detects common teolog paths.

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
DIM='\033[2m'
NC='\033[0m'

CMD="${1:-help}"
shift || true
LOG_FILE=""

print() {
  printf '%s\n' "$1"
}

print_color() {
  printf '%b\n' "$1"
}

print_blank() {
  printf '\n'
}

find_log_file() {
  local candidates=(
    "$HOME/.local/state/nvim/teolog.log"
    "$HOME/Library/Logs/teolog/teolog.json"
    "$HOME/.local/share/teolog/teolog.json"
  )
  local f
  for f in "${candidates[@]}"; do
    if [[ -f "$f" ]]; then
      print "$f"
      return
    fi
  done
  print ""
}

resolve_log_file() {
  local candidate="${1:-}"
  if [[ -n "$candidate" ]]; then
    LOG_FILE="$candidate"
  else
    LOG_FILE="$(find_log_file)"
  fi

  if [[ -z "$LOG_FILE" ]]; then
    print_color "${RED}Error: No log file found. Specify a path:${NC}"
    printf '  %s %s ... /path/to/logfile.json\n' "$0" "$CMD"
    exit 1
  fi

  if [[ ! -f "$LOG_FILE" ]]; then
    print_color "${RED}Error: Log file not found at $LOG_FILE${NC}"
    exit 1
  fi
}

require_jq() {
  if ! command -v jq &>/dev/null; then
    print_color "${YELLOW}jq is required. Install: brew install jq${NC}" >&2
    exit 1
  fi
}

jq_select_color() {
  local filter="$1"
  jq -C "$filter" "$LOG_FILE"
}

jq_field_counts() {
  local filter="$1"
  jq -r "$filter" "$LOG_FILE" | sort | uniq -c | sort -rn
}

jq_unique_count() {
  local filter="$1"
  jq -r "$filter" "$LOG_FILE" 2>/dev/null | sort -u | wc -l | tr -d ' '
}

trace_first_ts() {
  local tid="$1"
  jq -r "select(.trace_id == \"$tid\") | .ts" "$LOG_FILE" | head -1
}

trace_last_ts() {
  local tid="$1"
  jq -r "select(.trace_id == \"$tid\") | .ts" "$LOG_FILE" | tail -1
}

trace_has_error() {
  local tid="$1"
  jq -r "select(.trace_id == \"$tid\" and .lvl == \"error\") | .trace_id" "$LOG_FILE" | head -1
}

show_help() {
  print_color "${GREEN}teolog — NDJSON Log Analysis${NC}"
  print_blank
  printf 'Usage: %s <command> [command-args...] [log-file]\n' "$0"
  print_blank
  print 'Commands:'
  printf '  %btail%b [log-file]\n' "$BLUE" "$NC"
  print '      Stream logs in real-time (pretty-printed)'
  printf '  %berrors%b [log-file]\n' "$BLUE" "$NC"
  print '      Show all error events'
  printf '  %bwarnings%b [log-file]\n' "$BLUE" "$NC"
  print '      Show all warning events'
  printf '  %bspans%b [log-file]\n' "$BLUE" "$NC"
  print '      Show all span events (with duration)'
  printf '  %bslow%b [ms] [log-file]\n' "$BLUE" "$NC"
  print '      Show spans slower than N ms (default: 500)'
  printf '  %bproject%b NAME [log-file]\n' "$BLUE" "$NC"
  print '      Filter by project name'
  printf '  %bstats%b [log-file]\n' "$BLUE" "$NC"
  print '      Show log statistics'
  printf '  %brecent%b [N] [log-file]\n' "$BLUE" "$NC"
  print '      Show last N logs (default: 20)'
  printf '  %btrace%b [ID] [log-file]\n' "$BLUE" "$NC"
  print '      Show all events for trace_id, or list recent traces when ID omitted'
  printf '  %bsearch%b TEXT [log-file]\n' "$BLUE" "$NC"
  print '      Full-text search across all fields'
  printf '  %bclear%b [log-file]\n' "$BLUE" "$NC"
  print '      Clear log file (with confirmation)'
  printf '  %bhelp%b           Show this help\n' "$BLUE" "$NC"
  print_blank
  print 'Log file is auto-detected when omitted.'
  print 'Examples:'
  printf '  %s errors /path/to/my.log\n' "$0"
  printf '  %s slow 200 /path/to/my.log\n' "$0"
  printf '  %s trace a1b2c3d4 /path/to/my.log\n' "$0"
  printf '  %s recent 50\n' "$0"
}

show_header() {
  local color="$1"
  local text="$2"
  print_color "${color}${text}${NC}"
}

show_stats() {
  show_header "$GREEN" '=== Log Statistics ==='
  print_blank
  show_header "$BLUE" 'Total events:'
  wc -l <"$LOG_FILE" | tr -d ' '
  print_blank
  show_header "$BLUE" 'By level:'
  jq_field_counts '.lvl'
  print_blank
  show_header "$BLUE" 'By project:'
  jq_field_counts '.project // "(none)"'
  print_blank
  show_header "$BLUE" 'Spans:'
  jq_field_counts 'select(.span != null) | .span' | head -15
  print_blank
  show_header "$BLUE" 'Avg span duration (ms):'
  jq -s '[.[] | select(.dur_ms != null) | .dur_ms] | if length > 0 then (add / length | floor) else 0 end' "$LOG_FILE"
  print_blank
  show_header "$BLUE" 'Unique traces:'
  jq_unique_count 'select(.trace_id != null) | .trace_id'
  print_blank
  show_header "$BLUE" 'By service:'
  jq_field_counts '.service // "(none)"'
}

show_recent_traces() {
  show_header "$BLUE" '=== Recent Traces ==='
  print_blank
  jq_field_counts 'select(.trace_id != null) | .trace_id' | head -20 \
    | while read -r count tid; do
      local first_ts last_ts has_error marker
      first_ts=$(trace_first_ts "$tid")
      last_ts=$(trace_last_ts "$tid")
      has_error=$(trace_has_error "$tid")
      if [[ -n "$has_error" ]]; then
        marker="${RED}ERR${NC}"
      else
        marker="${GREEN} OK${NC}"
      fi
      print_color "  $marker ${DIM}${tid:0:8}…${NC}  ${count} events  ${DIM}${first_ts:-?} → ${last_ts:-?}${NC}"
    done
  print_blank
  print_color "${DIM}Usage: $0 trace <trace_id> [log-file]${NC}"
}

show_trace() {
  local trace_id="$1"
  show_header "$BLUE" "=== Trace: ${trace_id:0:8}… ==="
  print_blank
  jq 'select(.trace_id != null and (.trace_id | startswith("'"$trace_id"'")))' "$LOG_FILE" \
    | jq -C -s 'sort_by(.ts) | .[]'
  print_blank
  print_color "${DIM}--- Trace Summary ---${NC}"
  jq -s "
    [.[] | select(.trace_id != null and (.trace_id | startswith(\"$trace_id\")))]
    | {
        events: length,
        spans: ([.[] | select(.span != null)] | length),
        errors: ([.[] | select(.lvl == \"error\")] | length),
        total_dur_ms: ([.[] | select(.dur_ms != null) | .dur_ms] | add // 0),
        services: ([.[] | select(.service != null) | .service] | unique),
        projects: ([.[] | select(.project != null) | .project] | unique)
      }
  " "$LOG_FILE" | jq -C '.'
}

cmd_tail() {
  resolve_log_file "${1:-}"
  show_header "$GREEN" "Streaming: $LOG_FILE"
  print_color "${DIM}Ctrl+C to stop${NC}"
  tail -f "$LOG_FILE" | jq -C '.'
}

cmd_errors() {
  resolve_log_file "${1:-}"
  show_header "$RED" '=== Errors ==='
  jq_select_color 'select(.lvl == "error")'
}

cmd_warnings() {
  resolve_log_file "${1:-}"
  show_header "$YELLOW" '=== Warnings ==='
  jq_select_color 'select(.lvl == "warn")'
}

cmd_spans() {
  resolve_log_file "${1:-}"
  show_header "$BLUE" '=== Spans ==='
  jq_select_color 'select(.span != null)'
}

cmd_slow() {
  local threshold=500
  if [[ $# -ge 1 && "$1" =~ ^[0-9]+$ ]]; then
    threshold="$1"
    shift
  fi
  resolve_log_file "${1:-}"
  show_header "$YELLOW" "=== Slow Spans (> ${threshold}ms) ==="
  jq_select_color "select(.dur_ms != null and .dur_ms > $threshold)"
}

cmd_project() {
  local project="${1:?Usage: $0 project NAME [log-file]}"
  shift
  resolve_log_file "${1:-}"
  show_header "$BLUE" "=== Project: $project ==="
  jq_select_color "select(.project == \"$project\")"
}

cmd_stats() {
  resolve_log_file "${1:-}"
  show_stats
}

cmd_recent() {
  local count=20
  if [[ $# -ge 1 && "$1" =~ ^[0-9]+$ ]]; then
    count="$1"
    shift
  fi
  resolve_log_file "${1:-}"
  show_header "$GREEN" "=== Last $count Events ==="
  tail -"$count" "$LOG_FILE" | jq -C '.'
}

cmd_trace() {
  local trace_id=""
  if [[ $# -ge 1 ]]; then
    if [[ -f "$1" ]]; then
      resolve_log_file "$1"
    else
      trace_id="$1"
      shift
      resolve_log_file "${1:-}"
    fi
  else
    resolve_log_file
  fi

  if [[ -z "$trace_id" ]]; then
    show_recent_traces
  else
    show_trace "$trace_id"
  fi
}

cmd_search() {
  local query="${1:?Usage: $0 search TEXT [log-file]}"
  shift
  resolve_log_file "${1:-}"
  show_header "$BLUE" "=== Search: $query ==="
  jq_select_color "select(. | tostring | test(\"$query\"; \"i\"))"
}

cmd_clear() {
  resolve_log_file "${1:-}"
  print_color "${RED}Warning: This will delete all logs in $LOG_FILE${NC}"
  read -rp 'Are you sure? (yes/no): ' confirm
  if [[ "$confirm" == 'yes' ]]; then
    >"$LOG_FILE"
    print_color "${GREEN}Cleared.${NC}"
  else
    print 'Cancelled.'
  fi
}

require_jq

case "$CMD" in
  tail) cmd_tail "$@" ;;
  errors) cmd_errors "$@" ;;
  warnings) cmd_warnings "$@" ;;
  spans) cmd_spans "$@" ;;
  slow) cmd_slow "$@" ;;
  project) cmd_project "$@" ;;
  stats) cmd_stats "$@" ;;
  recent) cmd_recent "$@" ;;
  trace) cmd_trace "$@" ;;
  search) cmd_search "$@" ;;
  clear) cmd_clear "$@" ;;
  help|--help|-h) show_help ;;
  *)
    print_color "${RED}Unknown command: $CMD${NC}"
    print_blank
    show_help
    exit 1
    ;;
esac
