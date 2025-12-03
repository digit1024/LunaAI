# Build Dependencies for LunaAI

This document lists all dependencies needed to build and run LunaAI on Pop OS (Ubuntu-based) Linux systems.

## Analysis Method

The dependencies were discovered by:
1. Analyzing the compiled executable using `ldd` to detect linked libraries
2. Identifying which packages provide those libraries using `dpkg -S`
3. Adding build-time dependencies based on the project's requirements (Rust, COSMIC framework, WGPU, audio, etc.)

---

## Runtime Dependencies (Linked Libraries)

These libraries are linked directly to the executable and must be present at runtime:

| Library | Package | Description |
|---------|---------|-------------|
| `libasound.so.2` | `libasound2t64:amd64` | ALSA audio library (for audio playback) |
| `libcrypto.so.3` | `libssl3t64:amd64` | OpenSSL cryptographic library |
| `libc.so.6` | `libc6:amd64` | GNU C library |
| `libgcc_s.so.1` | `libgcc-s1:amd64` | GCC support library |
| `liblzma.so.5` | `liblzma5:amd64` | LZMA compression library |
| `libm.so.6` | `libc6:amd64` | Math library (part of glibc) |
| `libssl.so.3` | `libssl3t64:amd64` | OpenSSL SSL/TLS library |
| `libxkbcommon.so.0` | `libxkbcommon0:amd64` | Keyboard handling library (for Wayland/X11) |

**Note:** `ld-linux-x86-64.so.2` is the dynamic linker and is part of the base system.

---

## Build Dependencies

### Essential Build Tools

```bash
# Rust toolchain (via rustup or apt)
rustc          # Rust compiler
cargo          # Rust package manager

# Build essentials
build-essential    # GCC, G++, make, etc.
gcc               # C compiler
g++               # C++ compiler
pkg-config        # Package configuration tool
cmake             # Build system (may be needed by some Rust crates)
ninja-build       # Build system (may be needed by some Rust crates)
```

### System Library Development Packages

```bash
# Wayland (for COSMIC desktop framework)
libwayland-dev
libwayland-client-dev
libwayland-server-dev
libwayland-egl1-mesa-dev

# X11 (fallback/window system support)
libx11-dev
libx11-xcb-dev
libxcb1-dev
libxcb-render0-dev
libxcb-render-util0-dev
libxcb-xfixes0-dev
libxkbcommon-dev

# DBus (for desktop integration and configuration)
libdbus-1-dev

# OpenSSL (for HTTPS/TLS - reqwest, MCP)
libssl-dev

# ALSA (for audio playback - rodio, symphonia)
libasound2-dev

# Graphics/Mesa (for WGPU graphics rendering)
libegl1-mesa-dev
libgles2-mesa-dev
libvulkan-dev
vulkan-tools

# Other dependencies that may be required
libpkg-config-dev  # Sometimes needed for pkg-config
```

### Optional but Recommended

```bash
# Git (required for libcosmic dependency from GitHub)
git

# Development tools
curl              # For downloading Rust toolchain
wget              # Alternative download tool
```

---

## Complete Installation Command

For **Pop OS / Ubuntu**, you can install all build dependencies with:

```bash
sudo apt update
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
```

Then install Rust toolchain:

```bash
# Using rustup (recommended)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Or using apt (may have older version)
sudo apt install -y rustc cargo
```

---

## Runtime Dependencies Installation

For runtime, install:

```bash
sudo apt install -y \
    libasound2t64 \
    libssl3t64 \
    libc6 \
    libgcc-s1 \
    liblzma5 \
    libxkbcommon0
```

**Note:** Most of these are already present on a typical Pop OS installation, but they're listed here for completeness and for minimal/Docker environments.

---

## Verification

After installation, verify the setup:

```bash
# Check Rust
rustc --version
cargo --version

# Check pkg-config can find libraries
pkg-config --modversion wayland-client
pkg-config --modversion dbus-1
pkg-config --modversion alsa
pkg-config --modversion openssl

# Build the project
cd /path/to/LunaAI
unset ARGV0  # Important for cargo to work properly
cargo build --release
```

---

## Notes

1. **ARGV0 Unset**: As per project requirements, always unset `ARGV0` before running cargo commands to avoid proxy errors.

2. **libcosmic**: The project uses `libcosmic` from GitHub (Pop OS COSMIC desktop framework), which is built during compilation and doesn't require a system package.

3. **SQLite**: The project uses `rusqlite` with the `bundled` feature, so no system SQLite development package is needed.

4. **Bundled Dependencies**: Some dependencies like SQLite are bundled in the Rust crates, reducing the need for additional system packages.

---

## Dependency Summary by Category

### Core Runtime
- `libc6` - C standard library
- `libgcc-s1` - GCC support
- `liblzma5` - Compression

### Audio
- `libasound2t64` - ALSA audio

### Security/Networking
- `libssl3t64` - SSL/TLS support

### Desktop/Windowing
- `libxkbcommon0` - Keyboard handling

### Build Tools (compile-time only)
- Rust toolchain (rustc, cargo)
- C/C++ toolchain (gcc, g++)
- pkg-config, cmake, ninja-build

### Build Libraries (compile-time only)
- Wayland development libraries
- X11 development libraries
- DBus development library
- OpenSSL development library
- ALSA development library
- Mesa/Vulkan development libraries

---

*Last updated: Generated from executable analysis of `cosmic_llm` release binary*



