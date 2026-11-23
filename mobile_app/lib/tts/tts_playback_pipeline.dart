import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/foundation.dart';

import 'tts_audio_sink.dart';

/// Produces the audio for one text chunk, or `null` when there is nothing to
/// play for it. Throw [TtsSynthesisFailure] to signal a retryable error.
typedef TtsSynthesizer = Future<TtsAudioClip?> Function(String chunk, int index);

/// Reports a chunk that could not be synthesised after all attempts.
typedef TtsChunkFailureHandler = void Function(int index);

/// A synthesis error, optionally retryable.
class TtsSynthesisFailure implements Exception {
  const TtsSynthesisFailure(this.message, {this.retryable = false});

  final String message;
  final bool retryable;

  @override
  String toString() => 'TtsSynthesisFailure($message, retryable: $retryable)';
}

/// Feeds a [TtsAudioSink] from a [TtsSynthesizer] while keeping synthesis of
/// the next chunks in flight, so playback never waits on the network.
///
/// One instance serves exactly one [run]; create a new pipeline per utterance.
class TtsPlaybackPipeline {
  TtsPlaybackPipeline({
    required TtsSynthesizer synthesize,
    required TtsAudioSink sink,
    this.prefetchDepth = 2,
    this.maxAttempts = 2,
    this.retryDelay = const Duration(milliseconds: 300),
    this.onChunkFailed,
  })  : _synthesize = synthesize,
        _sink = sink;

  /// How many chunks may be synthesising at once, counted from the chunk that
  /// is about to play.
  final int prefetchDepth;

  /// Total attempts per chunk, including the first one.
  final int maxAttempts;

  final Duration retryDelay;

  final TtsChunkFailureHandler? onChunkFailed;

  final TtsSynthesizer _synthesize;
  final TtsAudioSink _sink;
  final Map<int, Future<TtsAudioClip?>> _inflight = {};

  bool _cancelled = false;

  bool get isCancelled => _cancelled;

  /// Synthesises and plays every chunk in order, returning once the last one
  /// has finished playing.
  Future<void> run(List<String> chunks) async {
    if (chunks.isEmpty) return;
    try {
      for (var i = 0; i < chunks.length; i++) {
        if (_cancelled) break;
        _ensureRequested(chunks, i);
        final clip = await _inflight[i];
        if (_cancelled) break;
        if (clip == null) {
          onChunkFailed?.call(i);
          continue;
        }
        await _sink.add(clip);
      }
      if (!_cancelled) {
        await _sink.drain();
      }
    } finally {
      _inflight.clear();
    }
  }

  /// Aborts synthesis, stops playback and releases the sink's buffers.
  Future<void> cancel() async {
    _cancelled = true;
    await _sink.stop();
  }

  void _ensureRequested(List<String> chunks, int from) {
    if (_cancelled) return;
    final last = math.min(from + prefetchDepth, chunks.length - 1);
    for (var i = from; i <= last; i++) {
      _inflight.putIfAbsent(i, () => _fetch(chunks[i], i));
    }
  }

  Future<TtsAudioClip?> _fetch(String chunk, int index) async {
    var attempt = 0;
    while (true) {
      attempt++;
      if (_cancelled) return null;
      try {
        return await _synthesize(chunk, index);
      } on TtsSynthesisFailure catch (failure) {
        debugPrint('TTS: chunk $index attempt $attempt: ${failure.message}');
        if (!failure.retryable) return null;
      } catch (e) {
        debugPrint('TTS: chunk $index attempt $attempt failed: $e');
      }
      if (_cancelled || attempt >= maxAttempts) return null;
      await Future<void>.delayed(retryDelay);
    }
  }
}
