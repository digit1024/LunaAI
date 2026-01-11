#!/bin/bash
# Fix for wear package missing namespace
# This script patches the wear package's build.gradle to add the required namespace

WEAR_PACKAGE_PATH="$HOME/.pub-cache/hosted/pub.dev/wear-1.1.0/android/build.gradle"

if [ -f "$WEAR_PACKAGE_PATH" ]; then
    # Check if namespace is already added
    if ! grep -q "namespace" "$WEAR_PACKAGE_PATH"; then
        echo "Applying namespace fix to wear package..."
        # Add namespace after 'android {' line
        sed -i '/^android {/a\\tnamespace '\''com.mjohnsullivan.flutterwear.wear'\''' "$WEAR_PACKAGE_PATH"
        echo "✓ Namespace added to wear package"
    else
        echo "✓ Namespace already present in wear package"
    fi
    
    # Fix Kotlin version (must be 1.5.20+ for newer AGP)
    if grep -q "kotlin_version = '1.5.10'" "$WEAR_PACKAGE_PATH"; then
        echo "Updating Kotlin version in wear package..."
        sed -i "s/kotlin_version = '1.5.10'/kotlin_version = '1.9.0'/" "$WEAR_PACKAGE_PATH"
        echo "✓ Kotlin version updated to 1.9.0"
    elif grep -q "kotlin_version = '1.5" "$WEAR_PACKAGE_PATH"; then
        echo "✓ Kotlin version already updated"
    fi
    
    # Add JVM target compatibility if missing
    if ! grep -q "kotlinOptions" "$WEAR_PACKAGE_PATH"; then
        echo "Adding JVM target compatibility to wear package..."
        # Add after defaultConfig block
        sed -i '/defaultConfig {/,/}/ {
            /}/ a\
\t}\
\tcompileOptions {\
\t\tsourceCompatibility JavaVersion.VERSION_17\
\t\ttargetCompatibility JavaVersion.VERSION_17\
\t}\
\tkotlinOptions {\
\t\tjvmTarget = '\''17'\''\
\t}
        }' "$WEAR_PACKAGE_PATH"
        echo "✓ JVM target compatibility added"
    else
        echo "✓ JVM target compatibility already present"
    fi
else
    echo "⚠ Wear package not found at $WEAR_PACKAGE_PATH"
    echo "  Run 'flutter pub get' first"
fi

