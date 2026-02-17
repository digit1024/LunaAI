#!/usr/bin/env sh
# Source env vars and run trigger (Luna WebSocket client).
# Usage: ./run_trigger.sh [path-to-env-file] [trigger-args...]
# Default env file: telegram-bridge.env in this script's directory.
# Example: ./run_trigger.sh "Hello"   or   ./run_trigger.sh -p openai -f prompt.txt

set -e
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ENV_FILE="$SCRIPT_DIR/telegram-bridge.env"
# If first arg looks like an env file (path with / or ending in .env), use it and shift.
if [ -n "$1" ]; then
  case "$1" in
    */*|*.env) ENV_FILE="$1"; shift ;;
  esac
fi

if [ -f "$ENV_FILE" ]; then
  set -a
  # shellcheck source=/dev/null
  . "$ENV_FILE"
  set +a
else
  echo "No env file at: $ENV_FILE" >&2
  echo "Create it from telegram-bridge.env.example or pass a path: $0 /path/to/env [trigger-args...]" >&2
  exit 1
fi

missing=""
[ -z "${LUNA_ADDRESS:-}" ]  && missing="${missing} LUNA_ADDRESS"
[ -z "${LUNA_API_KEY:-}" ]  && missing="${missing} LUNA_API_KEY"

if [ -n "$missing" ]; then
  echo "Missing required env vars:$missing" >&2
  echo "Set them in $ENV_FILE (or export before running)." >&2
  exit 1
fi

cd "$SCRIPT_DIR"
exec python3 trigger.py "$@"
