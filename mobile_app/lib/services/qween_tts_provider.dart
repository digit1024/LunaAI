import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_sound/flutter_sound.dart';

import '../core/config/qween_tts_preferences.dart';
import '../data/http/qween_tts_client.dart';
import '../tts/tts_text_chunker.dart';
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

  /// PCM16 mono bytes produced per millisecond at 24 kHz.
  static const double _pcmBytesPerMs = kQweenTtsSampleRate * 2 / 1000;

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
        try {
          await _playSingleChunk(
            apiKey: apiKey,
            voice: qweenPrefs.voice,
            text: chunks[i],
            isLast: i == chunks.length - 1,
          );
        } catch (e, st) {
          debugPrint('Qween TTS: chunk ${i + 1}/${chunks.length} failed: $e\n$st');
        }
      }
    } finally {
      if (!_stopped) {
        _callOnComplete();
      }
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
    int chunkPcmBytes = 0;
    DateTime? playbackStartTime;

    _client = QweenTtsClient(apiKey: apiKey, voice: voice);

    final playbackDone = Completer<void>();
    bool playbackCompleted = false;

    // Helper to mark this audio chunk as finished playing.
    void completePlayback() {
      if (!playbackCompleted && !_stopped) {
        playbackCompleted = true;
        if (!playbackDone.isCompleted) {
          playbackDone.complete();
        }
      }
    }

    try {
      var gotAudio = false;

      await for (final pcmChunk in _client!.synthesizeStream(text)) {
        if (_stopped) {
          playbackDone.complete();
          return;
        }

        if (!_streamStarted) {
          playbackStartTime = DateTime.now();
          await _player!.startPlayerFromStream(
            codec: Codec.pcm16,
            numChannels: 1,
            sampleRate: kQweenTtsSampleRate,
            interleaved: true,
            bufferSize: 8192,
            // flutter_sound startPlayerFromStream only has onBufferUnderflow (no whenFinished)
            onBufferUnderflow: () {
              if (!_allChunksFed || _stopped || playbackCompleted) return;
              completePlayback();
            },
          );
          _streamStarted = true;
        }

        await _player!.feedUint8FromStream(Uint8List.fromList(pcmChunk));
        chunkPcmBytes += pcmChunk.length;
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

      // Mark that all chunks have been fed to the player
      _allChunksFed = true;

      await _waitForChunkPlayback(
        pcmBytes: chunkPcmBytes,
        playbackStartTime: playbackStartTime,
        playbackDone: playbackDone,
        label: isLast ? 'last' : 'intermediate',
      );
      completePlayback();
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

    bool playbackCompleted = false;

    void completePlayback() {
      if (!playbackCompleted && !_stopped) {
        playbackCompleted = true;
        if (!playbackDone.isCompleted) {
          playbackDone.complete();
        }
      }
    }

    try {
      final url = await _client!.synthesizeUrl(text);
      if (_stopped || url == null) {
        playbackDone.complete();
        return;
      }

      final wavBytes = await _client!.downloadWav(url);
      if (_stopped || wavBytes == null || wavBytes.isEmpty) {
        playbackDone.complete();
        return;
      }

      final pcmData = wavBytes.length > 44
          ? Uint8List.sublistView(wavBytes, 44)
          : wavBytes;

      _allChunksFed = false;
      final playbackStartTime = DateTime.now();
      await _player!.startPlayerFromStream(
        codec: Codec.pcm16,
        numChannels: 1,
        sampleRate: kQweenTtsSampleRate,
        interleaved: true,
        bufferSize: 8192,
        onBufferUnderflow: () {
          if (!_allChunksFed || _stopped || playbackCompleted) return;
          completePlayback();
        },
      );
      _streamStarted = true;

      await _player!.feedUint8FromStream(pcmData);
      _allChunksFed = true;

      await _waitForChunkPlayback(
        pcmBytes: pcmData.length,
        playbackStartTime: playbackStartTime,
        playbackDone: playbackDone,
        label: isLast ? 'url-last' : 'url-intermediate',
      );
      completePlayback();
    } catch (e) {
      debugPrint('Qween TTS URL fallback error: $e');
      playbackDone.complete();
    } finally {
      _client?.dispose();
      _client = null;
    }
  }

  /// Waits until buffered PCM audio should have finished playing.
  ///
  /// [onBufferUnderflow] is unreliable on Android (flutter_sound #1058) and can
  /// fire early while audio is still playing. We always wait at least the
  /// estimated remaining playback time based on bytes fed vs elapsed time.
  Future<void> _waitForChunkPlayback({
    required int pcmBytes,
    required DateTime? playbackStartTime,
    required Completer<void> playbackDone,
    required String label,
  }) async {
    if (pcmBytes <= 0) {
      return;
    }

    final elapsedMs = playbackStartTime != null
        ? DateTime.now().difference(playbackStartTime).inMilliseconds
        : 0;
    final waitMs = _remainingPlaybackMs(pcmBytes, elapsedMs);
    final totalMs = _calculateTotalDurationMillis(pcmBytes);

    debugPrint(
      'Qween TTS: wait $label total=${totalMs}ms elapsed=${elapsedMs}ms remaining=${waitMs}ms',
    );

    // Always wait the estimated remaining time. onBufferUnderflow can fire early
    // on Android while audio is still buffered (flutter_sound #1058).
    await Future<void>.delayed(Duration(milliseconds: waitMs));
  }

  int _remainingPlaybackMs(int pcmBytes, int elapsedSincePlaybackStartMs) {
    final playedBytes =
        (elapsedSincePlaybackStartMs * _pcmBytesPerMs).round().clamp(0, pcmBytes);
    final remainingBytes = pcmBytes - playedBytes;
    return _calculateTotalDurationMillis(remainingBytes) + 500;
  }

  /// Calculates the total duration of PCM data in milliseconds.
  /// PCM is 16-bit (2 bytes/sample), 1 channel, 24kHz.
  int _calculateTotalDurationMillis(int pcmBytes) {
    if (pcmBytes <= 0) return 0;
    // 16-bit PCM = 2 bytes per sample
    final samples = pcmBytes / 2;
    // duration (seconds) = samples / sample_rate
    final durationSec = samples / kQweenTtsSampleRate;
    return (durationSec * 1000).toInt();
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
