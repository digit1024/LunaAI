import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_sound/flutter_sound.dart';

import '../core/config/qween_tts_preferences.dart';
import '../data/http/qween_tts_client.dart';
import '../utils/platform_utils.dart';
import 'tts_provider.dart';

/// Qween (Alibaba Qwen) TTS provider with real-time streaming playback.
///
/// Uses flutter_sound's startPlayerFromStream + feedUint8FromStream to play
/// PCM chunks as they arrive from the SSE stream — no buffering delay.
///
/// Splits text into chunks ≤ 600 chars (API limit) and plays them sequentially.
class QweenTtsProvider implements TtsProvider {
  QweenTtsProvider(this._ref);

  final Ref _ref;

  QweenTtsClient? _client;
  FlutterSoundPlayer? _player;
  VoidCallback? _pendingOnComplete;
  bool _stopped = false;
  bool _playerOpen = false;
  bool _streamStarted = false;
  bool _allChunksFed = false;

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

    await _ensurePlayerOpen();

    final chunks = chunkTextForTts(text);
    if (chunks.isEmpty) {
      _callOnComplete();
      return;
    }

    debugPrint('Qween TTS: ▶ voice="${qweenPrefs.voice}" ${chunks.length} chunk(s)');

    try {
      for (var i = 0; i < chunks.length && !_stopped; i++) {
        await _playSingleChunk(
          apiKey: apiKey,
          voice: qweenPrefs.voice,
          text: chunks[i],
          isLast: i == chunks.length - 1,
        );
      }
    } catch (e) {
      debugPrint('Qween TTS error: $e');
      _callOnComplete();
    }
  }

  /// Plays one text chunk and returns when playback finishes.
  Future<void> _playSingleChunk({
    required String apiKey,
    required String voice,
    required String text,
    required bool isLast,
  }) async {
    _streamStarted = false;
    _allChunksFed = false;

    _client = QweenTtsClient(apiKey: apiKey, voice: voice);

    final playbackDone = Completer<void>();

    try {
      var gotAudio = false;

      await for (final pcmChunk in _client!.synthesizeStream(text)) {
        if (_stopped) {
          playbackDone.complete();
          return;
        }

        if (!_streamStarted) {
          await _player!.startPlayerFromStream(
            codec: Codec.pcm16,
            numChannels: 1,
            sampleRate: kQweenTtsSampleRate,
            interleaved: true,
            bufferSize: 8192,
            onBufferUnderflow: () {
              if (_allChunksFed && !_stopped) {
                if (!playbackDone.isCompleted) playbackDone.complete();
                if (isLast) _callOnComplete();
              }
            },
          );
          _streamStarted = true;
        }

        await _player!.feedUint8FromStream(Uint8List.fromList(pcmChunk));
        gotAudio = true;
      }

      if (_stopped) {
        playbackDone.complete();
        return;
      }

      if (!gotAudio) {
        debugPrint('Qween TTS: SSE gave no audio, trying URL fallback...');
        await _playChunkFromUrl(
          apiKey,
          voice,
          text,
          playbackDone: playbackDone,
          isLast: isLast,
        );
        return;
      }

      _allChunksFed = true;
      await playbackDone.future;
    } finally {
      _client?.dispose();
      _client = null;
      await _stopStream();
    }
  }

  Future<void> _playChunkFromUrl(
    String apiKey,
    String voice,
    String text, {
    required Completer<void> playbackDone,
    required bool isLast,
  }) async {
    _client = QweenTtsClient(apiKey: apiKey, voice: voice);

    try {
      final url = await _client!.synthesizeUrl(text);
      if (_stopped || url == null) {
        playbackDone.complete();
        if (isLast) _callOnComplete();
        return;
      }

      final wavBytes = await _client!.downloadWav(url);
      if (_stopped || wavBytes == null || wavBytes.isEmpty) {
        playbackDone.complete();
        if (isLast) _callOnComplete();
        return;
      }

      final pcmData = wavBytes.length > 44
          ? Uint8List.sublistView(wavBytes, 44)
          : wavBytes;

      _allChunksFed = false;
      await _player!.startPlayerFromStream(
        codec: Codec.pcm16,
        numChannels: 1,
        sampleRate: kQweenTtsSampleRate,
        interleaved: true,
        bufferSize: 8192,
        onBufferUnderflow: () {
          if (_allChunksFed && !_stopped) {
            if (!playbackDone.isCompleted) playbackDone.complete();
            if (isLast) _callOnComplete();
          }
        },
      );
      _streamStarted = true;

      await _player!.feedUint8FromStream(pcmData);
      _allChunksFed = true;
      await playbackDone.future;
    } catch (e) {
      debugPrint('Qween TTS URL fallback error: $e');
      playbackDone.complete();
      if (isLast) _callOnComplete();
    } finally {
      _client?.dispose();
      _client = null;
    }
  }

  Future<void> _ensurePlayerOpen() async {
    _player ??= FlutterSoundPlayer();
    if (!_playerOpen) {
      await _player!.openPlayer();
      _playerOpen = true;
    }
  }

  Future<void> _stopStream() async {
    if (_streamStarted) {
      try {
        await _player?.stopPlayer();
      } catch (_) {}
      _streamStarted = false;
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
    await _stopStream();
    _callOnComplete();
  }

  void dispose() {
    _client?.dispose();
    _client = null;
    if (_playerOpen) {
      _player?.closePlayer();
      _playerOpen = false;
    }
    _player = null;
  }
}

final qweenTtsProvider = Provider<QweenTtsProvider>((ref) {
  final provider = QweenTtsProvider(ref);
  ref.onDispose(() => provider.dispose());
  return provider;
});
