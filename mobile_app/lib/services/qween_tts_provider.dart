import 'dart:async';

import 'package:audioplayers/audioplayers.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../core/config/qween_tts_preferences.dart';
import '../data/http/qween_tts_client.dart';
import '../utils/platform_utils.dart';
import '../utils/wav_utils.dart';
import 'tts_provider.dart';

/// Qween (Alibaba Qwen) TTS provider.
/// Uses DashScope API, plays audio via audioplayers.
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
    final apiKey =
        await _ref.read(qweenTtsPreferencesProvider.notifier).getApiKey();

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
        debugPrint('Qween TTS: player completed');
        if (!_stopped) {
          _pendingOnComplete?.call();
          _pendingOnComplete = null;
        }
      });

    try {
      // Strategy 1: Non-streaming (full response) - most reliable
      debugPrint('Qween TTS: requesting non-streaming audio...');
      final audioBytes = await _client!.synthesize(text);

      if (_stopped) {
        _callOnComplete();
        return;
      }

      if (audioBytes != null && audioBytes.isNotEmpty) {
        debugPrint('Qween TTS: playing ${audioBytes.length} bytes');
        await _player!.stop();
        await _player!.play(BytesSource(audioBytes));
        return; // onComplete fires via onPlayerComplete listener
      }

      // Strategy 2: Streaming fallback - buffer all chunks then play
      debugPrint('Qween TTS: non-streaming failed, trying streaming...');
      _client = QweenTtsClient(
        apiKey: apiKey,
        voice: qweenPrefs.voice,
        instructions: qweenPrefs.instructions,
      );

      final chunks = <int>[];
      await for (final chunk in _client!.synthesizeStream(text)) {
        if (_stopped) {
          _callOnComplete();
          return;
        }
        chunks.addAll(chunk);
      }

      if (_stopped || chunks.isEmpty) {
        debugPrint('Qween TTS: no streaming audio received (${chunks.length} bytes)');
        _callOnComplete();
        return;
      }

      // Wrap PCM in WAV header for playback
      debugPrint('Qween TTS: wrapping ${chunks.length} PCM bytes in WAV');
      final wavBytes = pcmToWav(chunks, sampleRate: kQweenTtsSampleRate);
      await _player!.stop();
      await _player!.play(BytesSource(wavBytes));
    } catch (e) {
      debugPrint('Qween TTS error: $e');
      _callOnComplete();
    } finally {
      _client?.dispose();
      _client = null;
    }
  }

  void _callOnComplete() {
    _pendingOnComplete?.call();
    _pendingOnComplete = null;
  }

  @override
  Future<void> stop() async {
    _stopped = true;
    _client?.cancel();
    _client?.dispose();
    _client = null;
    await _player?.stop();
    _callOnComplete();
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
