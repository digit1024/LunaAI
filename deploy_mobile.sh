#!/usr/bin/env zsh

# Luna Mobile - Production/Release Deployment Script
# Deploys the Flutter mobile app to a connected device in release mode

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
PHONE="📱"
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
PLATFORM_ARG=""
if [ "$1" = "--android" ] || [ "$1" = "-a" ]; then
    PLATFORM_ARG="android"
elif [ "$1" = "--ios" ] || [ "$1" = "-i" ]; then
    PLATFORM_ARG="ios"
elif [ "$1" = "--help" ] || [ "$1" = "-h" ]; then
    echo "Usage: $0 [OPTIONS]"
    echo ""
    echo "Deploy Luna Mobile app to connected device in release mode"
    echo ""
    echo "Options:"
    echo "  --android, -a    Force Android deployment"
    echo "  --ios, -i        Force iOS deployment"
    echo "  --help, -h       Show this help message"
    echo ""
    echo "If no platform is specified, the script will auto-detect from connected devices."
    exit 0
fi

echo "${BLUE}${ROCKET} Luna Mobile - Production Deployment${NC}\n"

# Check if Flutter is installed
if ! command -v flutter &> /dev/null; then
    echo "${RED}${CROSS} Flutter is not installed or not in PATH${NC}"
    exit 1
fi

echo "${GREEN}${CHECK} Flutter found${NC}"

# Navigate to mobile app directory
cd "${MOBILE_APP_DIR}" || exit 1

# Check for connected devices
echo "\n${BLUE}${PHONE} Checking for connected devices...${NC}"
set +e  # Temporarily disable exit on error for grep
DEVICES=$(flutter devices --machine 2>/dev/null | grep -E 'device|emulator' || true)
set -e  # Re-enable exit on error

if [ -z "$DEVICES" ]; then
    echo "${RED}${CROSS} No devices or emulators found${NC}"
    echo "${YELLOW}Please connect a device or start an emulator${NC}"
    exit 1
fi

# Display connected devices
echo "${GREEN}${CHECK} Connected devices:${NC}"
flutter devices

# Determine platform
PLATFORM=""
if [ -n "$PLATFORM_ARG" ]; then
    PLATFORM="$PLATFORM_ARG"
    echo "${BLUE}${ARROW} Platform specified: ${PLATFORM}${NC}"
else
    # Auto-detect platform from connected devices
    set +e  # Temporarily disable exit on error for grep
    DEVICE_OUTPUT=$(flutter devices 2>/dev/null || true)
    ANDROID_COUNT=$(echo "$DEVICE_OUTPUT" | grep -ic 'android\|chrome' || echo "0")
    IOS_COUNT=$(echo "$DEVICE_OUTPUT" | grep -ic 'ios\|iphone\|ipad' || echo "0")
    set -e  # Re-enable exit on error
    
    if [ "$ANDROID_COUNT" -gt 0 ] && [ "$IOS_COUNT" -eq 0 ]; then
        PLATFORM="android"
    elif [ "$IOS_COUNT" -gt 0 ] && [ "$ANDROID_COUNT" -eq 0 ]; then
        PLATFORM="ios"
    elif [ "$ANDROID_COUNT" -gt 0 ] && [ "$IOS_COUNT" -gt 0 ]; then
        echo "${YELLOW}⚠ Multiple platforms detected. Defaulting to Android.${NC}"
        echo "${YELLOW}   Use --android or --ios to specify explicitly${NC}"
        PLATFORM="android"
    else
        echo "${RED}${CROSS} Could not detect platform from connected devices${NC}"
        echo "${YELLOW}   Use --android or --ios to specify platform${NC}"
        exit 1
    fi
fi

# Verify device is available for selected platform
set +e  # Temporarily disable exit on error for grep
if [ "$PLATFORM" = "android" ]; then
    if ! flutter devices | grep -qi 'android\|chrome'; then
        echo "${RED}${CROSS} No Android device found${NC}"
        exit 1
    fi
elif [ "$PLATFORM" = "ios" ]; then
    if ! flutter devices | grep -qi 'ios\|iphone\|ipad'; then
        echo "${RED}${CROSS} No iOS device found${NC}"
        exit 1
    fi
fi
set -e  # Re-enable exit on error

echo "\n${BLUE}${ARROW} Deploying to: ${PLATFORM}${NC}\n"

# Clean previous builds
echo "${BLUE}${BUILD} Cleaning previous builds...${NC}"
flutter clean

# Get dependencies
echo "${BLUE}${BUILD} Getting dependencies...${NC}"
flutter pub get

# Build and deploy based on platform
if [ "$PLATFORM" = "android" ]; then
    echo "\n${BLUE}${BUILD} Building Android release APK...${NC}"
    flutter build apk --release
    
    echo "\n${BLUE}${ROCKET} Installing to Android device...${NC}"
    flutter install --release
    
    APK_PATH="build/app/outputs/flutter-apk/app-release.apk"
    if [ -f "$APK_PATH" ]; then
        echo "\n${GREEN}${CHECK} APK built successfully: ${APK_PATH}${NC}"
        echo "${GREEN}${CHECK} App installed to device${NC}"
    else
        echo "${RED}${CROSS} APK not found at expected path${NC}"
        exit 1
    fi
    
elif [ "$PLATFORM" = "ios" ]; then
    echo "\n${BLUE}${BUILD} Building iOS release...${NC}"
    flutter build ios --release --no-codesign
    
    echo "\n${BLUE}${ROCKET} Installing to iOS device...${NC}"
    flutter install --release
    
    echo "\n${GREEN}${CHECK} iOS app built and installed${NC}"
    echo "${YELLOW}⚠ Note: For App Store distribution, you'll need to archive and sign in Xcode${NC}"
fi

echo "\n${GREEN}${CHECK}${CHECK}${CHECK} Deployment complete!${NC}\n"

