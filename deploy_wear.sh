#!/usr/bin/env zsh

# Luna Wear OS - Production/Release Deployment Script
# Deploys the Flutter Wear OS app to a connected watch in release mode

set -e  # Exit on error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Icons
CHECK="✓"
CROSS="✗"
ARROW="→"
WATCH="⌚"
BUILD="🔨"
ROCKET="🚀"

# Script directory (works in both zsh and bash)
if [ -n "$ZSH_VERSION" ]; then
    SCRIPT_DIR="$(cd "$(dirname "${(%):-%x}")" && pwd)"
elif [ -n "$BASH_VERSION" ]; then
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
else
    SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
fi
MOBILE_APP_DIR="${SCRIPT_DIR}/mobile_app"

# Parse command line arguments
DEBUG_MODE=""
if [ "$1" = "--debug" ] || [ "$1" = "-d" ]; then
    DEBUG_MODE="--debug"
    echo "${YELLOW}⚠ Building in DEBUG mode${NC}"
elif [ "$1" = "--help" ] || [ "$1" = "-h" ]; then
    echo "Usage: $0 [OPTIONS]"
    echo ""
    echo "Deploy Luna Wear OS app to connected watch in release mode"
    echo ""
    echo "Options:"
    echo "  --debug, -d      Build in debug mode (faster, larger APK)"
    echo "  --help, -h       Show this help message"
    echo ""
    echo "Requirements:"
    echo "  - Wear OS device connected via ADB (USB or WiFi)"
    echo "  - ADB debugging enabled on watch"
    exit 0
fi

echo "${BLUE}${ROCKET} Luna Wear OS - Production Deployment${NC}\n"

# Check if Flutter is installed
if ! command -v flutter &> /dev/null; then
    echo "${RED}${CROSS} Flutter is not installed or not in PATH${NC}"
    exit 1
fi

echo "${GREEN}${CHECK} Flutter found${NC}"

# Check if ADB is installed
if ! command -v adb &> /dev/null; then
    echo "${RED}${CROSS} ADB is not installed or not in PATH${NC}"
    exit 1
fi

echo "${GREEN}${CHECK} ADB found${NC}"

# Navigate to mobile app directory
cd "${MOBILE_APP_DIR}" || exit 1

# Check for connected Wear OS devices
echo "\n${BLUE}${WATCH} Checking for connected Wear OS devices...${NC}"
set +e  # Temporarily disable exit on error
DEVICES=$(adb devices | grep -v "List of devices" | grep -v "^$" | grep "device$")
set -e  # Re-enable exit on error

if [ -z "$DEVICES" ]; then
    echo "${RED}${CROSS} No devices found${NC}"
    echo "${YELLOW}Please connect your Wear OS watch via ADB:${NC}"
    echo "  1. Enable Developer Options on watch"
    echo "  2. Enable ADB Debugging"
    echo "  3. Connect via USB or WiFi (adb connect <watch-ip>:5555)"
    exit 1
fi

# Display connected devices
echo "${GREEN}${CHECK} Connected devices:${NC}"
adb devices -l | grep -v "List of devices" | grep -v "^$"

# Fix wear package if needed
echo "\n${BLUE}${BUILD} Patching wear package (if needed)...${NC}"
if [ -f "./fix_wear_package.sh" ]; then
    ./fix_wear_package.sh
fi

# Get dependencies
echo "\n${BLUE}${BUILD} Getting dependencies...${NC}"
flutter pub get

# Build Wear OS APK
if [ -n "$DEBUG_MODE" ]; then
    echo "\n${BLUE}${BUILD} Building Wear OS debug APK...${NC}"
    flutter build apk -t wear/lib/main.dart --debug
    APK_PATH="build/app/outputs/flutter-apk/app-debug.apk"
else
    echo "\n${BLUE}${BUILD} Building Wear OS release APK...${NC}"
    flutter build apk -t wear/lib/main.dart --release
    APK_PATH="build/app/outputs/flutter-apk/app-release.apk"
fi

if [ ! -f "$APK_PATH" ]; then
    echo "${RED}${CROSS} APK not found at expected path: ${APK_PATH}${NC}"
    exit 1
fi

echo "${GREEN}${CHECK} APK built successfully: ${APK_PATH}${NC}"

# Get APK size
APK_SIZE=$(du -h "$APK_PATH" | cut -f1)
echo "${GREEN}${CHECK} APK size: ${APK_SIZE}${NC}"

# Install to device
echo "\n${BLUE}${ROCKET} Installing to Wear OS device...${NC}"
adb install -r "$APK_PATH"

echo "\n${GREEN}${CHECK}${CHECK}${CHECK} Deployment complete!${NC}"
echo "${GREEN}${WATCH} Luna Wear is now installed on your watch${NC}\n"

# Optionally launch the app
echo "${BLUE}${ARROW} Launching app...${NC}"
adb shell am start -n com.luna.mobile.wear/.MainActivity 2>/dev/null || true

echo ""


