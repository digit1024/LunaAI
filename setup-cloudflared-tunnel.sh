#!/bin/zsh
# Setup script for Cloudflare Tunnel with LunaAI

set -e

echo "🔧 Setting up Cloudflare Tunnel for LunaAI"
echo ""

# Check if cloudflared is installed
if ! command -v cloudflared &> /dev/null; then
    echo "❌ cloudflared is not installed"
    echo "   Install it from: https://github.com/cloudflare/cloudflared/releases"
    exit 1
fi

echo "✅ cloudflared found: $(cloudflared --version | head -n1)"
echo ""

# Step 1: Create tunnel (if it doesn't exist)
echo "📝 Step 1: Creating tunnel 'LUNAAI'..."
echo "   If the tunnel already exists, this will show an error (that's OK)"
cloudflared tunnel create LUNAAI 2>&1 | grep -v "already exists" || true
echo ""

# Step 2: Get tunnel ID
echo "📝 Step 2: Getting tunnel ID..."
TUNNEL_ID=$(cloudflared tunnel list | grep LUNAAI | awk '{print $1}' | head -n1)
if [ -z "$TUNNEL_ID" ]; then
    echo "❌ Failed to get tunnel ID. Please create the tunnel manually:"
    echo "   cloudflared tunnel create LUNAAI"
    exit 1
fi
echo "   Tunnel ID: $TUNNEL_ID"
echo ""

# Step 3: Copy credentials
echo "📝 Step 3: Setting up credentials..."
CRED_FILE="$HOME/.cloudflared/$TUNNEL_ID.json"
if [ -f "$CRED_FILE" ]; then
    echo "   ✅ Credentials file already exists: $CRED_FILE"
else
    echo "   ⚠️  Credentials file not found. Please run:"
    echo "   cloudflared tunnel create LUNAAI"
    exit 1
fi

# Create symlink for credentials.json
ln -sf "$CRED_FILE" "$HOME/.cloudflared/credentials.json"
echo "   ✅ Created symlink: $HOME/.cloudflared/credentials.json -> $CRED_FILE"
echo ""

# Step 4: Copy config
echo "📝 Step 4: Setting up configuration..."
CONFIG_SOURCE="$HOME/proj/LunaAI/cloudflared-config.yml"
CONFIG_TARGET="$HOME/.cloudflared/config.yml"

if [ -f "$CONFIG_SOURCE" ]; then
    cp "$CONFIG_SOURCE" "$CONFIG_TARGET"
    echo "   ✅ Copied config to: $CONFIG_TARGET"
else
    echo "   ⚠️  Config file not found at: $CONFIG_SOURCE"
    exit 1
fi
echo ""

# Step 5: Route DNS (optional - user needs to do this in dashboard)
echo "📝 Step 5: DNS Configuration"
echo "   ⚠️  You need to configure DNS in Cloudflare dashboard:"
echo ""
echo "   For WebSocket endpoint:"
echo "   - Go to Cloudflare Dashboard > Your Domain (digit1024.win) > DNS"
echo "   - Add CNAME record:"
echo "     Name: luna"
echo "     Target: $TUNNEL_ID.cfargotunnel.com"
echo "     Proxy: ON (orange cloud)"
echo ""
echo "   For API endpoint (optional):"
echo "   - Add CNAME record:"
echo "     Name: luna-api"
echo "     Target: $TUNNEL_ID.cfargotunnel.com"
echo "     Proxy: ON (orange cloud)"
echo ""
echo "   OR use this command to route automatically:"
echo "   cloudflared tunnel route dns LUNAAI luna.digit1024.win"
echo "   cloudflared tunnel route dns LUNAAI luna-api.digit1024.win"
echo ""

# Step 6: Test configuration
echo "📝 Step 6: Validating configuration..."
cloudflared tunnel --config "$CONFIG_TARGET" ingress validate
echo ""

echo "✅ Setup complete!"
echo ""
echo "🚀 To start the tunnel, run:"
echo "   cloudflared tunnel --config $CONFIG_TARGET run LUNAAI"
echo ""
echo "   Or simply:"
echo "   cloudflared tunnel run LUNAAI"
echo ""









