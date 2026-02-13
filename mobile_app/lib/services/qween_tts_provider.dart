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
///
/// Strategy:
/// 1. SSE streaming — buffer PCM chunks, wrap in WAV, play.
///    This gives lower latency (audio starts generating while we buffer).
/// 2. Fallback: non-streaming URL — get WAV URL, download, play.
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
      _callOnComplete();
      return;
    }

    _initPlayer();

    _client = QweenTtsClient(
      apiKey: apiKey,
      voice: qweenPrefs.voice,
      instructions: qweenPrefs.instructions,
    );

    try {
      // Strategy 1: SSE streaming — buffer PCM chunks, wrap in WAV, play
      debugPrint('Qween TTS: starting SSE stream...');
      final chunks = <int>[];

      await for (final chunk in _client!.synthesizeStream(text)) {
        if (_stopped) {
          _callOnComplete();
          return;
        }
        chunks.addAll(chunk);
      }

      if (_stopped) {
        _callOnComplete();
        return;
      }

      if (chunks.isNotEmpty) {
        debugPrint('Qween TTS: playing ${chunks.length} PCM bytes as WAV');
        final wavBytes = pcmToWav(chunks, sampleRate: kQweenTtsSampleRate);
        await _player!.stop();
        await _player!.play(BytesSource(wavBytes));
        return; // onComplete fires via onPlayerComplete listener
      }

      // Strategy 2: Fallback to non-streaming URL download
      debugPrint('Qween TTS: SSE gave no audio, trying URL fallback...');
      _client = QweenTtsClient(
        apiKey: apiKey,
        voice: qweenPrefs.voice,
        instructions: qweenPrefs.instructions,
      );

      final url = await _client!.synthesizeUrl(text);
      if (_stopped || url == null) {
        _callOnComplete();
        return;
      }

      final wavBytes = await _client!.downloadWav(url);
      if (_stopped || wavBytes == null || wavBytes.isEmpty) {
        _callOnComplete();
        return;
      }

      debugPrint('Qween TTS: playing downloaded WAV (${wavBytes.length} bytes)');
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

  void _initPlayer() {
    _player ??= AudioPlayer()
      ..setReleaseMode(ReleaseMode.stop)
      ..onPlayerComplete.listen((_) {
        debugPrint('Qween TTS: playback complete');
        if (!_stopped) {
          _callOnComplete();
        }
      });
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
