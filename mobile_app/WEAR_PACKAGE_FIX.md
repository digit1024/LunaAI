# Wear Package Build Fix

## Issue

The `wear` package (version 1.1.0) has compatibility issues with newer Android Gradle Plugin versions:
1. Missing `namespace` declaration (required by AGP 8.0+)
2. Kotlin version too old (1.5.10, needs 1.5.20+)
3. Missing JVM target compatibility settings

## Solution

A fix script automatically patches the wear package's build.gradle file.

### **Automatic Fix**

Run the fix script before building:

```bash
cd mobile_app
./fix_wear_package.sh
```

This script:
- ✅ Adds `namespace 'com.mjohnsullivan.flutterwear.wear'` to the wear package
- ✅ Updates Kotlin version from 1.5.10 to 1.9.0
- ✅ Adds JVM target compatibility (Java 17)

### **When to Run**

Run the fix script:
- After `flutter pub get` (if it overwrites the package)
- Before building the Wear OS app
- If you see namespace or Kotlin version errors

### **Manual Fix (if script doesn't work)**

If the script doesn't work, manually edit:
```
~/.pub-cache/hosted/pub.dev/wear-1.1.0/android/build.gradle
```

Add these changes:

1. **Add namespace:**
```gradle
android {
    namespace 'com.mjohnsullivan.flutterwear.wear'
    // ... rest of config
}
```

2. **Update Kotlin version:**
```gradle
buildscript {
    ext.kotlin_version = '1.9.0'  // Changed from 1.5.10
    // ...
}
```

3. **Add JVM target:**
```gradle
android {
    // ... existing config ...
    compileOptions {
        sourceCompatibility JavaVersion.VERSION_17
        targetCompatibility JavaVersion.VERSION_17
    }
    kotlinOptions {
        jvmTarget = '17'
    }
}
```

## Integration with Android Studio

### **Option 1: Run Script Before Building**

1. Open Terminal in Android Studio
2. Run: `./fix_wear_package.sh`
3. Build the app normally

### **Option 2: Add to Gradle Build**

You can add a Gradle task to automatically run the fix. Add to `android/build.gradle`:

```gradle
task fixWearPackage {
    doLast {
        exec {
            commandLine 'bash', '../fix_wear_package.sh'
        }
    }
}

preBuild.dependsOn fixWearPackage
```

### **Option 3: Post-Install Hook**

Add to `pubspec.yaml` (if supported in future Flutter versions):
```yaml
# Note: This is not currently supported, but you can use a script
```

## Verification

After running the fix, verify it worked:

```bash
grep -A 2 "namespace" ~/.pub-cache/hosted/pub.dev/wear-1.1.0/android/build.gradle
```

Should show:
```
android {
	namespace 'com.mjohnsullivan.flutterwear.wear'
```

## Notes

- ⚠️ **This fix is temporary** - The package will be overwritten on `flutter pub get`
- ✅ **Script is idempotent** - Safe to run multiple times
- 🔄 **Run after pub get** - Always run the script after `flutter pub get` or `flutter clean`

## Future Solution

The proper fix would be:
1. Fork the `wear` package
2. Apply fixes
3. Publish as `wear_fixed` or submit PR to original package
4. Update `pubspec.yaml` to use fixed version

For now, the script approach works well and is easy to maintain.






