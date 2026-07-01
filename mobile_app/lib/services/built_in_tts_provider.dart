import 'dart:async';

import 'package:flutter/foundation.dart';

import '../tts/tts_text_chunker.dart';
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
    final trimmed = text.trim();
    if (trimmed.isEmpty) {
      onComplete?.call();
      return;
    }

    await _ttsService.setLanguage(_getLanguage());

    final chunks = chunkTextForTts(
      trimmed,
      maxLen: kBuiltInTtsMaxInputLength,
    );
    if (chunks.isEmpty) {
      onComplete?.call();
      return;
    }

    for (var i = 0; i < chunks.length; i++) {
      final isLast = i == chunks.length - 1;
      final chunkDone = Completer<void>();

      await _ttsService.stop();
      await _ttsService.speak(
        chunks[i],
        onComplete: () {
          if (!chunkDone.isCompleted) {
            chunkDone.complete();
          }
          if (isLast) {
            onComplete?.call();
          }
        },
      );
      await chunkDone.future;
    }
  }

  @override
  Future<void> stop() async {
    await _ttsService.stop();
  }
}
