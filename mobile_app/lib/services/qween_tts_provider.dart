import 'dart:async';

import 'package:audioplayers/audioplayers.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../core/config/qween_tts_preferences.dart';
import '../data/http/qween_tts_client.dart';
import '../utils/platform_utils.dart';
import '../utils/wav_utils.dart';
import 'tts_provider.dart';

/// Qween (Alibaba Qwen) TTS provider with streaming support.
/// Uses DashScope API, buffers PCM chunks, plays via audioplayers.
class QweenTtsProvider implements TtsProvider {
  QweenTtsProvider(this._ref);

  final Ref _ref;

  QweenTtsClient? _client;
  AudioPlayer? _player;
  VoidCallback? _pendingOnComplete;
  bool _stopped = false;

  @override
  Future<void> speak(
    String text, {
    VoidCallback? onComplete,
  }) async {
    if (text.trim().isEmpty) {
      onComplete?.call();
      return;
    }

    if (!isMobile) {
      debugPrint('Qween TTS: Not available on desktop/web');
      onComplete?.call();
      return;
    }

    _stopped = false;
    _pendingOnComplete = onComplete;

    final qweenPrefs = _ref.read(qweenTtsPreferencesProvider);
    final apiKey = await _ref.read(qweenTtsPreferencesProvider.notifier).getApiKey();

    if (apiKey == null || apiKey.trim().isEmpty) {
      debugPrint('Qween TTS: No API key configured');
      _pendingOnComplete?.call();
      _pendingOnComplete = null;
      return;
    }

    _client = QweenTtsClient(
      apiKey: apiKey,
      voice: qweenPrefs.voice,
      instructions: qweenPrefs.instructions,
    );

    _player ??= AudioPlayer()
      ..setReleaseMode(ReleaseMode.stop)
      ..onPlayerComplete.listen((_) {
        if (!_stopped) {
          _pendingOnComplete?.call();
          _pendingOnComplete = null;
        }
      });

    try {
      final chunks = <int>[];
      await for (final chunk in _client!.synthesizeStream(text)) {
        if (_stopped) return;
        chunks.addAll(chunk);
      }

      if (_stopped) {
        _pendingOnComplete?.call();
        _pendingOnComplete = null;
        return;
      }
      if (chunks.isEmpty) {
        _pendingOnComplete?.call();
        _pendingOnComplete = null;
        return;
      }

      final wavBytes = pcmToWav(
        chunks,
        sampleRate: kQweenTtsSampleRate,
      );

      await _player!.stop();
      await _player!.play(BytesSource(wavBytes));
    } catch (e) {
      debugPrint('Qween TTS error: $e');
      _pendingOnComplete?.call();
      _pendingOnComplete = null;
    } finally {
      _client?.dispose();
      _client = null;
    }
  }

  @override
  Future<void> stop() async {
    _stopped = true;
    _client?.cancel();
    _client?.dispose();
    _client = null;
    await _player?.stop();
    _pendingOnComplete?.call();
    _pendingOnComplete = null;
  }

  void dispose() {
    _client?.dispose();
    _client = null;
    _player?.dispose();
    _player = null;
  }
}

final qweenTtsProvider = Provider<QweenTtsProvider>((ref) {
  final provider = QweenTtsProvider(ref);
  ref.onDispose(() => provider.dispose());
  return provider;
});
