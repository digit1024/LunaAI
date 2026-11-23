import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:luna_mobile/services/built_in_tts_provider.dart';
import 'package:luna_mobile/services/device_tts_engine.dart';

/// Lets pending microtasks and zero-delay timers run.
Future<void> _flush() => Future<void>.delayed(Duration.zero);

/// Long enough to be split into two chunks at [kBuiltInTtsMaxInputLength].
String _longText() => List.filled(900, 'word').join(' ');

class FakeEngine implements DeviceTtsEngine {
  FakeEngine({this.supportsUtteranceQueue = true});

  @override
  final bool supportsUtteranceQueue;

  final List<String> log = [];
  final List<String> spoken = [];
  final List<VoidCallback> _pending = [];

  bool queueing = false;
  String? language;
  int stopCount = 0;

  @override
  Future<void> setLanguage(String languageCode) async {
    language = languageCode;
    log.add('language:$languageCode');
  }

  @override
  Future<void> setUtteranceQueueing(bool enabled) async {
    queueing = enabled;
    log.add(enabled ? 'queue:on' : 'queue:off');
  }

  @override
  Future<void> speak(String text, {VoidCallback? onComplete}) async {
    spoken.add(text);
    log.add('speak');
    if (onComplete != null) _pending.add(onComplete);
  }

  @override
  Future<void> stop() async {
    stopCount++;
    log.add('stop');
    // Mirrors TtsService: pending callbacks are dropped, not fired.
    _pending.clear();
    if (queueing) {
      queueing = false;
      log.add('queue:off');
    }
  }

  /// Simulates the engine finishing its oldest queued utterance.
  void finishOldest() {
    if (_pending.isEmpty) return;
    _pending.removeAt(0)();
  }
}

void main() {
  group('BuiltInTtsProvider', () {
    test('appends every chunk to the engine queue instead of stopping', () async {
      final engine = FakeEngine();
      final provider = BuiltInTtsProvider(
        engine: engine,
        getLanguage: () => 'pl-PL',
      );
      var completed = 0;

      final run = provider.speak(_longText(), onComplete: () => completed++);
      await _flush();

      expect(engine.language, 'pl-PL');
      expect(engine.spoken, hasLength(2));
      for (final chunk in engine.spoken) {
        expect(chunk.length, lessThanOrEqualTo(3500));
      }
      // One initial flush only: no stop between the chunks, which is what used
      // to leave an audible gap.
      expect(engine.stopCount, 1);
      expect(engine.log, containsAllInOrder(['queue:on', 'speak', 'speak']));
      expect(completed, 0);

      engine.finishOldest();
      await _flush();
      expect(completed, 0);

      engine.finishOldest();
      await run;

      expect(completed, 1);
      expect(engine.queueing, isFalse);
      expect(engine.log.last, 'queue:off');
    });

    test('leaves the engine unqueued for a single chunk', () async {
      final engine = FakeEngine();
      final provider = BuiltInTtsProvider(
        engine: engine,
        getLanguage: () => 'en-US',
      );
      var completed = 0;

      final run = provider.speak('Short message.', onComplete: () => completed++);
      await _flush();

      expect(engine.spoken, ['Short message.']);
      expect(engine.log, isNot(contains('queue:on')));

      engine.finishOldest();
      await run;
      expect(completed, 1);
    });

    test('waits between chunks when the engine cannot queue', () async {
      final engine = FakeEngine(supportsUtteranceQueue: false);
      final provider = BuiltInTtsProvider(
        engine: engine,
        getLanguage: () => 'en-US',
      );
      var completed = 0;

      final run = provider.speak(_longText(), onComplete: () => completed++);
      await _flush();

      expect(engine.log, isNot(contains('queue:on')));
      expect(engine.spoken, hasLength(1));

      engine.finishOldest();
      await _flush();
      expect(engine.spoken, hasLength(2));

      engine.finishOldest();
      await run;
      expect(completed, 1);
    });

    test('stop reports completion once and halts the session', () async {
      final engine = FakeEngine();
      final provider = BuiltInTtsProvider(
        engine: engine,
        getLanguage: () => 'en-US',
      );
      var completed = 0;

      final run = provider.speak(_longText(), onComplete: () => completed++);
      await _flush();
      expect(engine.spoken, hasLength(2));

      await provider.stop();
      await run;

      expect(completed, 1);
      expect(engine.queueing, isFalse);
      expect(engine.stopCount, 2);
    });

    test('a newer speak supersedes the previous one without double reporting',
        () async {
      final engine = FakeEngine();
      final provider = BuiltInTtsProvider(
        engine: engine,
        getLanguage: () => 'en-US',
      );
      var firstCompleted = 0;
      var secondCompleted = 0;

      final superseded = provider.speak(
        _longText(),
        onComplete: () => firstCompleted++,
      );
      await _flush();
      expect(engine.spoken, hasLength(2));

      final current = provider.speak(
        'Short one.',
        onComplete: () => secondCompleted++,
      );
      await _flush();
      await superseded;

      expect(engine.spoken, hasLength(3));
      expect(firstCompleted, 0);

      engine.finishOldest();
      await current;

      expect(secondCompleted, 1);
    });

    test('completes even when the engine never reports an utterance', () async {
      final engine = FakeEngine();
      final provider = BuiltInTtsProvider(
        engine: engine,
        getLanguage: () => 'en-US',
        watchdogGrace: Duration.zero,
      );
      var completed = 0;

      // "Short message." estimates to 2s of speech, so the watchdog fires soon.
      await provider
          .speak('Short message.', onComplete: () => completed++)
          .timeout(const Duration(seconds: 15));

      expect(completed, 1);
    });

    test('empty text completes immediately', () async {
      final engine = FakeEngine();
      final provider = BuiltInTtsProvider(
        engine: engine,
        getLanguage: () => 'en-US',
      );
      var completed = 0;

      await provider.speak('   ', onComplete: () => completed++);

      expect(completed, 1);
      expect(engine.log, isEmpty);
    });
  });

  group('BuiltInTtsProvider.estimateSpeechDuration', () {
    test('scales with text length', () {
      expect(BuiltInTtsProvider.estimateSpeechDuration(0), Duration.zero);
      expect(
        BuiltInTtsProvider.estimateSpeechDuration(100),
        const Duration(seconds: 10),
      );
      expect(
        BuiltInTtsProvider.estimateSpeechDuration(3501),
        const Duration(seconds: 351),
      );
    });
  });
}
