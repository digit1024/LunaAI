#!/usr/bin/env sh
# Source env vars and start the Telegram bridge.
# Usage: ./run_telegram_bridge.sh [path-to-env-file]
# Default env file: telegram-bridge.env in this script's directory.

set -e
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ENV_FILE="${1:-$SCRIPT_DIR/telegram-bridge.env}"

if [ -f "$ENV_FILE" ]; then
  set -a
  # shellcheck source=/dev/null
  . "$ENV_FILE"
  set +a
else
  echo "No env file at: $ENV_FILE" >&2
  echo "Create it from telegram-bridge.env.example or pass a path: $0 /path/to/env" >&2
  exit 1
fi

missing=""
[ -z "${TELEGRAM_BOT_TOKEN:-}" ] && missing="${missing} TELEGRAM_BOT_TOKEN"
[ -z "${LUNA_ADDRESS:-}" ]       && missing="${missing} LUNA_ADDRESS"
[ -z "${LUNA_API_KEY:-}" ]      && missing="${missing} LUNA_API_KEY"

if [ -n "$missing" ]; then
  echo "Missing required env vars:$missing" >&2
  echo "Set them in $ENV_FILE (or export before running)." >&2
  exit 1
fi

cd "$SCRIPT_DIR"
exec python3 telegram_bridge.py
