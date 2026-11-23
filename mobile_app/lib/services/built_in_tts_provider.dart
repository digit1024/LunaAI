import 'dart:async';

import 'package:flutter/foundation.dart';

import '../tts/tts_text_chunker.dart';
import 'device_tts_engine.dart';
import 'tts_provider.dart';

/// Built-in TTS provider backed by the device speech engine.
///
/// On Android the chunks of a long message are appended to the engine's own
/// utterance queue (`QUEUE_ADD`) so it reads them back-to-back, without the
/// stop/start gap that flushing between chunks introduces. Platforms without
/// queue support speak chunks one at a time.
class BuiltInTtsProvider implements TtsProvider {
  BuiltInTtsProvider({
    required DeviceTtsEngine engine,
    required String Function() getLanguage,
    this.watchdogGrace = const Duration(seconds: 30),
  })  : _engine = engine,
        _getLanguage = getLanguage;

  final DeviceTtsEngine _engine;
  final String Function() _getLanguage;

  /// Extra time allowed on top of the estimated speech duration before a
  /// session whose engine callbacks never arrive is given up on.
  final Duration watchdogGrace;

  /// Conservative speech rate, used only to bound a stuck session.
  static const double _charsPerSecond = 10;

  Completer<void>? _session;
  Timer? _watchdog;
  VoidCallback? _pendingOnComplete;
  int _generation = 0;
  int _expected = 0;
  int _finished = 0;

  @override
  Future<void> speak(String text, {VoidCallback? onComplete}) async {
    final trimmed = text.trim();
    if (trimmed.isEmpty) {
      onComplete?.call();
      return;
    }

    final generation = ++_generation;
    _pendingOnComplete = onComplete;
    _cancelWatchdog();
    _abandonSession();
    await _engine.stop();
    await _engine.setLanguage(_getLanguage());

    final chunks = chunkTextForTts(
      trimmed,
      maxLen: kBuiltInTtsMaxInputLength,
    );
    if (chunks.isEmpty) {
      _reportComplete(generation);
      return;
    }
    if (generation != _generation) return;

    final session = Completer<void>();
    _session = session;
    _expected = chunks.length;
    _finished = 0;

    final queueing = _engine.supportsUtteranceQueue && chunks.length > 1;
    if (queueing) await _engine.setUtteranceQueueing(true);
    final watchdog = _armWatchdog(session, chunks);

    try {
      for (final chunk in chunks) {
        if (generation != _generation) return;
        final utteranceDone = Completer<void>();
        await _engine.speak(
          chunk,
          onComplete: () {
            _onUtteranceComplete(generation);
            if (!utteranceDone.isCompleted) utteranceDone.complete();
          },
        );
        // Without queue support the engine flushes on every utterance, so the
        // next chunk must wait for the current one. Racing the session keeps a
        // dropped engine callback from wedging the loop.
        if (!queueing) {
          await Future.any([utteranceDone.future, session.future]);
        }
      }
      await session.future;
    } finally {
      watchdog.cancel();
      if (_watchdog == watchdog) _watchdog = null;
      // A newer speak() or stop() owns the engine and the completion report
      // from here on.
      if (generation == _generation && queueing) {
        await _engine.setUtteranceQueueing(false);
      }
      _reportComplete(generation);
    }
  }

  @override
  Future<void> stop() async {
    final generation = ++_generation;
    _cancelWatchdog();
    final session = _session;
    _session = null;
    await _engine.stop();
    if (session != null && !session.isCompleted) session.complete();
    _reportComplete(generation);
  }

  /// Fires the pending completion callback at most once per session.
  void _reportComplete(int generation) {
    if (generation != _generation) return;
    _session = null;
    final callback = _pendingOnComplete;
    _pendingOnComplete = null;
    callback?.call();
  }

  void _onUtteranceComplete(int generation) {
    if (generation != _generation) return;
    _finished++;
    final session = _session;
    if (session != null && _finished >= _expected && !session.isCompleted) {
      session.complete();
    }
  }

  Timer _armWatchdog(Completer<void> session, List<String> chunks) {
    final charCount =
        chunks.fold<int>(0, (total, chunk) => total + chunk.length);
    final timeout = estimateSpeechDuration(charCount) + watchdogGrace;
    final watchdog = Timer(timeout, () {
      debugPrint(
        'TTS: built-in engine went quiet, giving up after '
        '${timeout.inSeconds}s',
      );
      _watchdog = null;
      if (!session.isCompleted) session.complete();
    });
    _watchdog = watchdog;
    return watchdog;
  }

  void _cancelWatchdog() {
    _watchdog?.cancel();
    _watchdog = null;
  }

  void _abandonSession() {
    final session = _session;
    _session = null;
    if (session != null && !session.isCompleted) session.complete();
  }

  @visibleForTesting
  static Duration estimateSpeechDuration(int charCount) =>
      Duration(seconds: (charCount / _charsPerSecond).ceil());
}
