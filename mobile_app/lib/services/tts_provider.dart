import 'package:flutter/foundation.dart';

/// Abstract interface for TTS providers (Built-in or Qween).
/// All TTS usage in the app goes through this interface.
abstract class TtsProvider {
  /// Speak the given text. Calls [onComplete] when done (or immediately on error).
  Future<void> speak(
    String text, {
    VoidCallback? onComplete,
  });

  /// Stop any ongoing speech and cancel any in-flight requests.
  Future<void> stop();
}
