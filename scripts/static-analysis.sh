#!/usr/bin/env zsh
set -euo pipefail
unset ARGV0
cd "$(dirname "$0")/.."

REPORT_DIR="audits"
mkdir -p "$REPORT_DIR"

echo "=== cargo check ==="
cargo check --workspace 2>&1 | tee "$REPORT_DIR/check.log"

echo "=== cargo clippy ==="
cargo clippy --workspace --all-targets --message-format=short 2>&1 \
  | tee "$REPORT_DIR/clippy.log"

echo "=== cargo audit ==="
cargo audit 2>&1 | tee "$REPORT_DIR/audit.log" || true

echo "=== Summary ==="
echo "Clippy warnings: $(grep -cE '\.rs:[0-9]+:[0-9]+: warning:' "$REPORT_DIR/clippy.log" || echo 0)"
echo "Audit vulnerabilities: $(grep -c '^Crate:' "$REPORT_DIR/audit.log" || echo 0)"
echo "Logs written to $REPORT_DIR/"
