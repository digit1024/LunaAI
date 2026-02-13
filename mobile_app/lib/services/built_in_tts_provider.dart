import 'package:flutter/foundation.dart';

import 'tts_provider.dart';
import 'tts_service.dart';

/// Built-in TTS provider wrapping [TtsService] (flutter_tts).
/// Uses [getLanguage] to obtain the current TTS language before speak.
class BuiltInTtsProvider implements TtsProvider {
  BuiltInTtsProvider({
    required TtsService ttsService,
    required String Function() getLanguage,
  })  : _ttsService = ttsService,
        _getLanguage = getLanguage;

  final TtsService _ttsService;
  final String Function() _getLanguage;

  @override
  Future<void> speak(
    String text, {
    VoidCallback? onComplete,
  }) async {
    await _ttsService.setLanguage(_getLanguage());
    await _ttsService.speak(text, onComplete: onComplete);
  }

  @override
  Future<void> stop() async {
    await _ttsService.stop();
  }
}
