import 'dart:io';
import 'package:flutter/foundation.dart';

/// Check if running on mobile platforms (Android/iOS)
bool get isMobile => !kIsWeb && (Platform.isAndroid || Platform.isIOS);

/// Check if running on desktop platforms (Linux/Windows/macOS)
bool get isDesktop => !kIsWeb && (Platform.isLinux || Platform.isWindows || Platform.isMacOS);

/// Check if running on web (Chrome, etc.)
bool get isWeb => kIsWeb;

/// Check if TTS/STT should be enabled
/// Only enabled on mobile platforms
bool get isVoiceEnabled => isMobile;




















