#!/usr/bin/env bash
# Build .deb packages for luna-ai-server, luna-ai-quick-setup, luna-thin-ui (x86_64).
# Run from repo root. Requires: cargo, python3, dpkg-deb.

set -e
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PACKAGING="$REPO_ROOT/packaging"
OUT="$REPO_ROOT/packaging/out"
rm -rf "$OUT"
mkdir -p "$OUT"

# --- 1) Luna AI Server ---
echo "=== Building luna-ai-server ==="
unset ARGV0
cargo build --release -p cosmic_llm
SERVER_STAGE="$PACKAGING/luna-ai-server/usr/bin"
mkdir -p "$SERVER_STAGE"
cp "$REPO_ROOT/target/release/cosmic_llm" "$SERVER_STAGE/"
chmod 755 "$SERVER_STAGE/cosmic_llm"

# --- 2) Luna AI Quick Setup ---
echo "=== Building luna-ai-quick-setup ==="
QS_STAGE="$PACKAGING/luna-ai-quick-setup/usr"
mkdir -p "$QS_STAGE/share/luna-ai-quick-setup" "$QS_STAGE/bin"
rsync -a --exclude='__pycache__' --exclude='*.pyc' "$REPO_ROOT/quick_setup/quick_setup" "$QS_STAGE/share/luna-ai-quick-setup/"
rm -rf "$QS_STAGE/share/luna-ai-quick-setup/quick_setup/__pycache__"
cp -r "$REPO_ROOT/quick_setup/catalog" "$REPO_ROOT/quick_setup/sample_data" "$REPO_ROOT/quick_setup/self_config" "$QS_STAGE/share/luna-ai-quick-setup/"
# Wrapper so luna_ai_quick_setup finds quick_setup under /usr/share
cat > "$QS_STAGE/bin/luna_ai_quick_setup" << 'WRAP'
#!/usr/bin/python3
import sys, os
_SHARE = "/usr/share/luna-ai-quick-setup"
if os.path.isdir(_SHARE):
    sys.path.insert(0, _SHARE)
    os.chdir(_SHARE)
from quick_setup.main import main
if __name__ == "__main__":
    main()
WRAP
chmod 755 "$QS_STAGE/bin/luna_ai_quick_setup"

# --- 3) Luna Thin UI ---
echo "=== Building luna-thin-ui ==="
unset ARGV0
cargo build --release -p luna_thin_ui
UI_BIN="$PACKAGING/luna-thin-ui/usr/bin"
UI_APPS="$PACKAGING/luna-thin-ui/usr/share/applications"
UI_ICONS="$PACKAGING/luna-thin-ui/usr/share/icons/hicolor/scalable/apps"
mkdir -p "$UI_BIN" "$UI_APPS" "$UI_ICONS"
cp "$REPO_ROOT/target/release/luna-thin" "$UI_BIN/"
chmod 755 "$UI_BIN/luna-thin"
# Desktop and icon already in packaging tree; ensure icon is present
cp "$REPO_ROOT/luna_thin_ui/res/com.github.digit1024.luna.svg" "$UI_ICONS/"
# Desktop file already created
test -f "$UI_APPS/com.github.digit1024.luna.desktop"

# --- Build .deb packages ---
echo "=== Creating .deb files ==="
for pkg in luna-ai-server luna-ai-quick-setup luna-thin-ui; do
    dpkg-deb -b "$PACKAGING/$pkg" "$OUT/${pkg}_0.1.0_amd64.deb"
    echo "  -> $OUT/${pkg}_0.1.0_amd64.deb"
done
echo "Done. Output in $OUT"
