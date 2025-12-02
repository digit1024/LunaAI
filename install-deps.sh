#!/bin/bash
# Installation script for LunaAI build dependencies
# For Pop OS / Ubuntu-based systems

set -e

echo "🔍 Installing build dependencies for LunaAI..."

# Update package list
sudo apt update

# Install all build dependencies
sudo apt install -y \
    build-essential \
    gcc \
    g++ \
    pkg-config \
    cmake \
    ninja-build \
    libwayland-dev \
    libwayland-client-dev \
    libwayland-server-dev \
    libwayland-egl1-mesa-dev \
    libx11-dev \
    libx11-xcb-dev \
    libxcb1-dev \
    libxcb-render0-dev \
    libxcb-render-util0-dev \
    libxcb-xfixes0-dev \
    libxkbcommon-dev \
    libdbus-1-dev \
    libssl-dev \
    libasound2-dev \
    libegl1-mesa-dev \
    libgles2-mesa-dev \
    libvulkan-dev \
    vulkan-tools \
    git \
    curl

echo "✅ Build dependencies installed!"

# Check if Rust is installed
if ! command -v rustc &> /dev/null; then
    echo "⚠️  Rust toolchain not found. Installing rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
    echo "✅ Rust toolchain installed!"
else
    echo "✅ Rust toolchain already installed: $(rustc --version)"
fi

# Install runtime dependencies (if not already present)
echo "🔍 Ensuring runtime dependencies are installed..."
sudo apt install -y \
    libasound2t64 \
    libssl3t64 \
    libc6 \
    libgcc-s1 \
    liblzma5 \
    libxkbcommon0

echo ""
echo "🎉 All dependencies installed successfully!"
echo ""
echo "Next steps:"
echo "  1. If Rust was just installed, run: source \$HOME/.cargo/env"
echo "  2. Navigate to the project directory"
echo "  3. Build the project: unset ARGV0 && cargo build --release"
echo ""


