#!/usr/bin/env bash
# teolog — Pretty-print NDJSON wide events from stdin or file
# Colorizes by level, shows span duration, context, and data.
#
# Usage:
#   tail -f logfile.json | logs-pretty.sh
#   logs-pretty.sh < logfile.json
#   cargo run 2>&1 | logs-pretty.sh

if ! command -v jq &>/dev/null; then
  printf 'jq is required. Install: brew install jq\n' >&2
  exit 1
fi

jq -R 'fromjson? | select(. != null) |
  # Extract timestamp HH:MM:SS
  (.ts | split(".")[0] | split("T")[1] // .ts) as $time |

  # Build context summary
  ([.ctx // {} | to_entries[] | "\(.key)=\(.value)"] | join(" ")) as $ctx_str |

  # Build data summary (skip nested objects, show scalars)
  ([.data // {} | to_entries[] |
    if (.value | type) == "object" or (.value | type) == "array"
    then "\(.key)=..."
    else "\(.key)=\(.value)"
    end
  ] | join(" ")) as $data_str |

  # Duration tag
  (if .dur_ms then " \u001b[90m(\(.dur_ms)ms)\u001b[0m" else "" end) as $dur |

  # Span tag
  (if .span then "\u001b[90m[\(.span)]\u001b[0m " else "" end) as $span |

  # Details line
  (if ($ctx_str + $data_str) != "" then
    " \u001b[90m{" + $ctx_str + (if $ctx_str != "" and $data_str != "" then " " else "" end) + $data_str + "}\u001b[0m"
  else "" end) as $details |

  # Error details
  (if (.err.message // .data.error.message) then " \u001b[31m(" + (.err.message // .data.error.message) + ")\u001b[0m" else "" end) as $err |

  # Project tag
  (if .project then "\u001b[90m" + .project + "\u001b[0m " else "" end) as $proj |

  # Colorize by level
  if .lvl == "error" then
    "\u001b[1;31mE\u001b[0m \u001b[90m" + $time + "\u001b[0m " + $proj + $span + "\u001b[31m" + .msg + "\u001b[0m" + $err + $dur + $details
  elif .lvl == "warn" then
    "\u001b[1;33mW\u001b[0m \u001b[90m" + $time + "\u001b[0m " + $proj + $span + "\u001b[33m" + .msg + "\u001b[0m" + $dur + $details
  elif .lvl == "info" then
    "\u001b[1;32mI\u001b[0m \u001b[90m" + $time + "\u001b[0m " + $proj + $span + .msg + $dur + $details
  elif .lvl == "debug" then
    "\u001b[36mD\u001b[0m \u001b[90m" + $time + "\u001b[0m " + $proj + $span + "\u001b[90m" + .msg + "\u001b[0m" + $dur + $details
  else
    "\u001b[90mT " + $time + " " + $proj + $span + .msg + $dur + $details + "\u001b[0m"
  end
'
