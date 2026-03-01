#!/usr/bin/env bash
# teolog — NDJSON wide-event log analysis utilities
# Works with any teolog-format NDJSON log file.
#
# Usage: analyze-logs.sh [command] [log-file]
# If log-file is omitted, uses the first argument after command or auto-detects.

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
DIM='\033[2m'
NC='\033[0m'

# Default log file: try common teolog locations
find_log_file() {
  local candidates=(
    "$HOME/.local/state/nvim/teolog.log"
    "$HOME/Library/Logs/teolog/teolog.json"
    "$HOME/.local/share/teolog/teolog.json"
  )
  for f in "${candidates[@]}"; do
    if [[ -f "$f" ]]; then
      echo "$f"
      return
    fi
  done
  echo ""
}

# Parse args: command [log-file]
CMD="${1:-help}"
shift || true
LOG_FILE="${1:-$(find_log_file)}"

if [[ "$CMD" != "help" && "$CMD" != "--help" && "$CMD" != "-h" && -z "$LOG_FILE" ]]; then
  echo -e "${RED}Error: No log file found. Specify a path:${NC}"
  echo "  $0 $CMD /path/to/logfile.json"
  exit 1
fi

if [[ "$CMD" != "help" && "$CMD" != "--help" && "$CMD" != "-h" && ! -f "$LOG_FILE" ]]; then
  echo -e "${RED}Error: Log file not found at $LOG_FILE${NC}"
  exit 1
fi

if ! command -v jq &>/dev/null; then
  echo -e "${YELLOW}jq is required. Install: brew install jq${NC}"
  exit 1
fi

show_help() {
  echo -e "${GREEN}teolog — NDJSON Log Analysis${NC}"
  echo ""
  echo "Usage: $0 <command> [log-file]"
  echo ""
  echo "Commands:"
  echo "  ${BLUE}tail${NC}           Stream logs in real-time (pretty-printed)"
  echo "  ${BLUE}errors${NC}         Show all error events"
  echo "  ${BLUE}warnings${NC}       Show all warning events"
  echo "  ${BLUE}spans${NC}          Show all span events (with duration)"
  echo "  ${BLUE}slow${NC} [ms]      Show spans slower than N ms (default: 500)"
  echo "  ${BLUE}project${NC} NAME   Filter by project name"
  echo "  ${BLUE}stats${NC}          Show log statistics"
  echo "  ${BLUE}recent${NC} [N]     Show last N logs (default: 20)"
  echo "  ${BLUE}search${NC} TEXT    Full-text search across all fields"
  echo "  ${BLUE}clear${NC}          Clear log file (with confirmation)"
  echo "  ${BLUE}help${NC}           Show this help"
  echo ""
  echo "The log file is auto-detected or can be passed as the last argument."
  echo "  $0 errors /path/to/my.log"
}

case "$CMD" in
  tail)
    echo -e "${GREEN}Streaming: $LOG_FILE${NC}"
    echo -e "${DIM}Ctrl+C to stop${NC}"
    tail -f "$LOG_FILE" | jq -C '.'
    ;;

  errors)
    echo -e "${RED}=== Errors ===${NC}"
    jq -C 'select(.lvl == "error")' "$LOG_FILE"
    ;;

  warnings)
    echo -e "${YELLOW}=== Warnings ===${NC}"
    jq -C 'select(.lvl == "warn")' "$LOG_FILE"
    ;;

  spans)
    echo -e "${BLUE}=== Spans ===${NC}"
    jq -C 'select(.span != null)' "$LOG_FILE"
    ;;

  slow)
    THRESHOLD="${2:-500}"
    echo -e "${YELLOW}=== Slow Spans (> ${THRESHOLD}ms) ===${NC}"
    jq -C "select(.dur_ms != null and .dur_ms > $THRESHOLD)" "$LOG_FILE"
    ;;

  project)
    PROJECT="${2:?Usage: $0 project NAME [log-file]}"
    echo -e "${BLUE}=== Project: $PROJECT ===${NC}"
    jq -C "select(.project == \"$PROJECT\")" "$LOG_FILE"
    ;;

  stats)
    echo -e "${GREEN}=== Log Statistics ===${NC}"
    echo ""
    echo -e "${BLUE}Total events:${NC}"
    wc -l <"$LOG_FILE" | tr -d ' '
    echo ""
    echo -e "${BLUE}By level:${NC}"
    jq -r '.lvl' "$LOG_FILE" | sort | uniq -c | sort -rn
    echo ""
    echo -e "${BLUE}By project:${NC}"
    jq -r '.project // "(none)"' "$LOG_FILE" | sort | uniq -c | sort -rn
    echo ""
    echo -e "${BLUE}Spans:${NC}"
    jq -r 'select(.span != null) | .span' "$LOG_FILE" | sort | uniq -c | sort -rn | head -15
    echo ""
    echo -e "${BLUE}Avg span duration (ms):${NC}"
    jq -s '[.[] | select(.dur_ms != null) | .dur_ms] | if length > 0 then (add / length | floor) else 0 end' "$LOG_FILE"
    ;;

  recent)
    COUNT="${2:-20}"
    echo -e "${GREEN}=== Last $COUNT Events ===${NC}"
    tail -"$COUNT" "$LOG_FILE" | jq -C '.'
    ;;

  search)
    QUERY="${2:?Usage: $0 search TEXT [log-file]}"
    echo -e "${BLUE}=== Search: $QUERY ===${NC}"
    jq -C "select(. | tostring | test(\"$QUERY\"; \"i\"))" "$LOG_FILE"
    ;;

  clear)
    echo -e "${RED}Warning: This will delete all logs in $LOG_FILE${NC}"
    read -rp "Are you sure? (yes/no): " confirm
    if [[ "$confirm" == "yes" ]]; then
      >"$LOG_FILE"
      echo -e "${GREEN}Cleared.${NC}"
    else
      echo "Cancelled."
    fi
    ;;

  help|--help|-h)
    show_help
    ;;

  *)
    echo -e "${RED}Unknown command: $CMD${NC}"
    echo ""
    show_help
    exit 1
    ;;
esac
