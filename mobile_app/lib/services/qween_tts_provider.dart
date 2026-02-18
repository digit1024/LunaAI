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
    int chunkPcmBytes = 0;
    DateTime? playbackStartTime;

    _client = QweenTtsClient(apiKey: apiKey, voice: voice);

    final playbackDone = Completer<void>();
    bool playbackCompleted = false;

    // Helper to complete playback and call onComplete if needed
    void completePlayback() {
      if (!playbackCompleted && !_stopped) {
        playbackCompleted = true;
        if (!playbackDone.isCompleted) {
          playbackDone.complete();
        }
        if (isLast) {
          _callOnComplete();
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
              // Intermediate chunks: only complete playbackDone so we can proceed to next chunk (no signal to app)
              if (!isLast) {
                playbackCompleted = true;
                if (!playbackDone.isCompleted) playbackDone.complete();
                return;
              }
              // Last chunk: complete playback and send onComplete signal to app
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

      // For intermediate chunks: wait for onBufferUnderflow (should fire when buffer drains)
      // For last chunk: onBufferUnderflow often never fires on Android (flutter_sound #1058),
      // so calculate duration from PCM bytes and delay that long to let it finish.
      if (isLast) {
        final totalDurationMs = _calculateTotalDurationMillis(chunkPcmBytes);
        final elapsedMs = playbackStartTime != null
            ? DateTime.now().difference(playbackStartTime).inMilliseconds
            : 0;
        
        // Remaining time is total duration minus what's played, with a safety buffer.
        final remainingMs = (totalDurationMs - elapsedMs).clamp(0, 999999) + 250;
        debugPrint('Qween TTS: total=${totalDurationMs}ms, elapsed=${elapsedMs}ms, delay=${remainingMs}ms');

        await Future<void>.delayed(Duration(milliseconds: remainingMs));
        completePlayback();
        await playbackDone.future; // Will resolve immediately
      } else {
        // Intermediate chunk: wait for onBufferUnderflow to fire (playback actually done)
        await playbackDone.future;
      }
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

    // Helper to complete playback and call onComplete if needed
    void completePlayback() {
      if (!playbackCompleted && !_stopped) {
        playbackCompleted = true;
        if (!playbackDone.isCompleted) {
          playbackDone.complete();
        }
        if (isLast) {
          _callOnComplete();
        }
      }
    }

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
      final playbackStartTime = DateTime.now();
      await _player!.startPlayerFromStream(
        codec: Codec.pcm16,
        numChannels: 1,
        sampleRate: kQweenTtsSampleRate,
        interleaved: true,
        bufferSize: 8192,
        onBufferUnderflow: () {
          if (!_allChunksFed || _stopped || playbackCompleted) return;
          // Intermediate: only complete playbackDone (no signal to app)
          if (!isLast) {
            playbackCompleted = true;
            if (!playbackDone.isCompleted) playbackDone.complete();
            return;
          }
          completePlayback();
        },
      );
      _streamStarted = true;

      await _player!.feedUint8FromStream(pcmData);
      _allChunksFed = true;
      
      // For intermediate chunks: wait for onBufferUnderflow (should fire when buffer drains)
      // For last chunk: use calculated duration to wait for playback to finish
      if (isLast) {
        final totalDurationMs = _calculateTotalDurationMillis(pcmData.length);
        final elapsedMs = DateTime.now().difference(playbackStartTime).inMilliseconds;
        final remainingMs = (totalDurationMs - elapsedMs).clamp(0, 999999) + 250;
        debugPrint('Qween TTS URL: total=${totalDurationMs}ms, elapsed=${elapsedMs}ms, delay=${remainingMs}ms');

        await Future<void>.delayed(Duration(milliseconds: remainingMs));
        completePlayback();
        await playbackDone.future;
      } else {
        // Intermediate chunk: wait for onBufferUnderflow to fire
        await playbackDone.future;
      }
    } catch (e) {
      debugPrint('Qween TTS URL fallback error: $e');
      playbackDone.complete();
      if (isLast) _callOnComplete();
    } finally {
      _client?.dispose();
      _client = null;
    }
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
