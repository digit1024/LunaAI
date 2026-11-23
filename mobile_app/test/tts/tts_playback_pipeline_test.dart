import 'dart:async';
import 'dart:math' as math;
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:luna_mobile/tts/tts_audio_sink.dart';
import 'package:luna_mobile/tts/tts_playback_pipeline.dart';

Uint8List _bytes(String text) => Uint8List.fromList(text.codeUnits);

String _text(TtsAudioClip clip) => String.fromCharCodes(clip.bytes);

TtsAudioClip _clip(String text) => TtsAudioClip(_bytes(text), 'wav');

/// Lets the event loop run pending microtasks and zero-delay timers.
Future<void> _flush() => Future<void>.delayed(Duration.zero);

class _Buffered {
  _Buffered(this.clip);

  final TtsAudioClip clip;
  final Completer<void> done = Completer<void>();
}

/// Models the [TtsAudioSink] contract: [add] returns once a clip is buffered
/// and blocks while all slots are occupied; playback is driven by the test.
class FakeSink implements TtsAudioSink {
  FakeSink({this.capacity = 2});

  final int capacity;
  final List<String> log = [];
  final List<_Buffered> _resident = [];
  final List<Completer<void>> _slotWaiters = [];

  int maxResident = 0;
  bool stopped = false;

  List<String> get played =>
      log.where((entry) => entry.startsWith('played:')).toList();

  List<String> get buffered =>
      log.where((entry) => entry.startsWith('buffered:')).toList();

  @override
  Future<void> add(TtsAudioClip clip) async {
    while (!stopped && _resident.length >= capacity) {
      final waiter = Completer<void>();
      _slotWaiters.add(waiter);
      await waiter.future;
    }
    if (stopped) return;
    _resident.add(_Buffered(clip));
    maxResident = math.max(maxResident, _resident.length);
    log.add('buffered:${_text(clip)}');
  }

  /// Simulates the oldest buffered clip finishing playback.
  void finishOldest() {
    if (_resident.isEmpty) return;
    final entry = _resident.removeAt(0);
    log.add('played:${_text(entry.clip)}');
    if (!entry.done.isCompleted) entry.done.complete();
    _notify();
  }

  @override
  Future<void> drain() async {
    while (_resident.isNotEmpty) {
      await _resident.first.done.future;
    }
  }

  @override
  Future<void> stop() async {
    stopped = true;
    for (final entry in _resident) {
      if (!entry.done.isCompleted) entry.done.complete();
    }
    _resident.clear();
    _notify();
  }

  @override
  Future<void> dispose() => stop();

  void _notify() {
    final waiters = List<Completer<void>>.of(_slotWaiters);
    _slotWaiters.clear();
    for (final waiter in waiters) {
      if (!waiter.isCompleted) waiter.complete();
    }
  }
}

void main() {
  group('TtsPlaybackPipeline', () {
    test('synthesises ahead instead of waiting for playback', () async {
      final synthLog = <String>[];
      final sink = FakeSink();
      final pipeline = TtsPlaybackPipeline(
        synthesize: (chunk, index) async {
          synthLog.add(chunk);
          return _clip(chunk);
        },
        sink: sink,
        prefetchDepth: 2,
        retryDelay: Duration.zero,
      );

      final run = pipeline.run(['a', 'b', 'c', 'd']);
      await _flush();

      // Every chunk is already synthesised although nothing has played yet —
      // this is the overlap that removes the gap between chunks.
      expect(synthLog, ['a', 'b', 'c', 'd']);
      expect(sink.buffered, ['buffered:a', 'buffered:b']);
      expect(sink.played, isEmpty);

      sink.finishOldest();
      await _flush();
      sink.finishOldest();
      await _flush();
      sink.finishOldest();
      await _flush();
      sink.finishOldest();
      await _flush();

      await run;
      expect(sink.played, [
        'played:a',
        'played:b',
        'played:c',
        'played:d',
      ]);
      expect(sink.maxResident, lessThanOrEqualTo(sink.capacity));
    });

    test('run does not complete before the last clip has played', () async {
      final sink = FakeSink();
      var completed = false;
      final pipeline = TtsPlaybackPipeline(
        synthesize: (chunk, index) async => _clip(chunk),
        sink: sink,
        retryDelay: Duration.zero,
      );

      final run = pipeline.run(['a', 'b']).then((_) => completed = true);
      await _flush();
      expect(completed, isFalse);

      sink.finishOldest();
      await _flush();
      expect(completed, isFalse);

      sink.finishOldest();
      await _flush();
      await run;
      expect(completed, isTrue);
    });

    test('reports and skips a chunk that produced no audio', () async {
      final sink = FakeSink();
      final failed = <int>[];
      final pipeline = TtsPlaybackPipeline(
        synthesize: (chunk, index) async => chunk == 'b' ? null : _clip(chunk),
        sink: sink,
        retryDelay: Duration.zero,
        onChunkFailed: failed.add,
      );

      final run = pipeline.run(['a', 'b', 'c']);
      await _flush();
      sink.finishOldest();
      await _flush();
      sink.finishOldest();
      await _flush();
      await run;

      expect(failed, [1]);
      expect(sink.played, ['played:a', 'played:c']);
    });

    test('retries a retryable synthesis failure', () async {
      final sink = FakeSink();
      var attemptsForB = 0;
      final failed = <int>[];
      final pipeline = TtsPlaybackPipeline(
        synthesize: (chunk, index) async {
          if (chunk == 'b') {
            attemptsForB++;
            if (attemptsForB == 1) {
              throw const TtsSynthesisFailure(
                '429 rate limited',
                retryable: true,
              );
            }
          }
          return _clip(chunk);
        },
        sink: sink,
        maxAttempts: 2,
        retryDelay: Duration.zero,
        onChunkFailed: failed.add,
      );

      final run = pipeline.run(['a', 'b']);
      await _flush();
      await _flush();
      sink.finishOldest();
      await _flush();
      sink.finishOldest();
      await _flush();
      await run;

      // One failed attempt plus the successful retry.
      expect(attemptsForB, 2);
      expect(failed, isEmpty);
      expect(sink.played, ['played:a', 'played:b']);
    });

    test('does not retry a permanent synthesis failure', () async {
      final sink = FakeSink();
      var attempts = 0;
      final failed = <int>[];
      final pipeline = TtsPlaybackPipeline(
        synthesize: (chunk, index) async {
          attempts++;
          throw const TtsSynthesisFailure('400 unsupported voice');
        },
        sink: sink,
        maxAttempts: 3,
        retryDelay: Duration.zero,
        onChunkFailed: failed.add,
      );

      final run = pipeline.run(['a', 'b']);
      await _flush();
      await run;

      expect(attempts, 2);
      expect(failed, [0, 1]);
      expect(sink.played, isEmpty);
    });

    test('cancel stops synthesis and playback', () async {
      final sink = FakeSink();
      final synthLog = <String>[];
      final pipeline = TtsPlaybackPipeline(
        synthesize: (chunk, index) async {
          synthLog.add(chunk);
          await Future<void>.delayed(const Duration(milliseconds: 5));
          return _clip(chunk);
        },
        sink: sink,
        prefetchDepth: 1,
        retryDelay: Duration.zero,
      );

      final run = pipeline.run(['a', 'b', 'c', 'd']);
      await _flush();
      expect(synthLog, ['a', 'b']);

      await pipeline.cancel();
      await run;

      // Only the already in-flight chunks were requested; nothing was played.
      expect(synthLog, ['a', 'b']);
      expect(sink.stopped, isTrue);
      expect(sink.played, isEmpty);
      expect(pipeline.isCancelled, isTrue);
    });

    test('cancel unblocks a run waiting for a free slot', () async {
      final sink = FakeSink(capacity: 1);
      final pipeline = TtsPlaybackPipeline(
        synthesize: (chunk, index) async => _clip(chunk),
        sink: sink,
        prefetchDepth: 1,
        retryDelay: Duration.zero,
      );

      final run = pipeline.run(['a', 'b', 'c']);
      await _flush();
      // One slot only: the run is parked inside sink.add for chunk b.
      expect(sink.buffered, ['buffered:a']);

      await pipeline.cancel();
      await run.timeout(const Duration(seconds: 2));

      expect(sink.played, isEmpty);
    });

    test('an empty chunk list plays nothing', () async {
      final sink = FakeSink();
      var calls = 0;
      final pipeline = TtsPlaybackPipeline(
        synthesize: (chunk, index) async {
          calls++;
          return _clip(chunk);
        },
        sink: sink,
      );

      await pipeline.run([]);

      expect(calls, 0);
      expect(sink.log, isEmpty);
    });
  });

  group('TtsAudioClip', () {
    test('maps known extensions to mime types', () {
      expect(_clip('x').mimeType, 'audio/wav');
      expect(TtsAudioClip(_bytes('x'), 'mp3').mimeType, 'audio/mpeg');
      expect(TtsAudioClip(_bytes('x'), 'MP3').mimeType, 'audio/mpeg');
      expect(TtsAudioClip(_bytes('x'), 'flac').mimeType, isNull);
    });
  });
}
