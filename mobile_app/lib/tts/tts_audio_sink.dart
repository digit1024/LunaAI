import 'dart:async';

import 'package:audioplayers/audioplayers.dart';
import 'package:flutter/foundation.dart';

/// A single synthesised piece of audio, ready for playback.
class TtsAudioClip {
  const TtsAudioClip(this.bytes, this.extension);

  final Uint8List bytes;

  /// File extension without a dot, e.g. `wav` or `mp3`.
  final String extension;

  String? get mimeType => switch (extension.toLowerCase()) {
        'wav' => 'audio/wav',
        'mp3' => 'audio/mpeg',
        'ogg' => 'audio/ogg',
        'aac' => 'audio/aac',
        'm4a' => 'audio/mp4',
        _ => null,
      };
}

/// Plays a sequence of [TtsAudioClip]s back-to-back.
///
/// A sink instance represents one playback session: after [stop] or [dispose]
/// it must not receive further clips.
abstract class TtsAudioSink {
  /// Buffers [clip] and plays it once every previously added clip has finished.
  ///
  /// Returns as soon as the clip is buffered — it does not wait for the audio
  /// itself. Implementations apply backpressure when their buffers are full.
  Future<void> add(TtsAudioClip clip);

  /// Completes when every clip passed to [add] has finished playing.
  Future<void> drain();

  /// Stops playback immediately. Pending [drain] futures complete.
  Future<void> stop();

  /// Releases the underlying players.
  Future<void> dispose();
}

typedef AudioPlayerFactory = AudioPlayer Function(String playerId);

AudioPlayer _defaultPlayerFactory(String playerId) =>
    AudioPlayer(playerId: playerId);

int _nextPlayerSuffix = 0;

/// Double-buffered [TtsAudioSink].
///
/// Two players alternate: while one is audible the other already has its
/// source set and is prepared, so handing over costs a single `resume()`
/// instead of a network round-trip plus a full media prepare.
class GaplessAudioSink implements TtsAudioSink {
  GaplessAudioSink({
    AudioPlayerFactory playerFactory = _defaultPlayerFactory,
    this.fallbackDuration = const Duration(seconds: 120),
    this.watchdogGrace = const Duration(seconds: 15),
  })  : _playerFactory = playerFactory,
        _idSuffix = _nextPlayerSuffix++;

  /// Assumed clip length when the player cannot report one, used only to bound
  /// a stuck playback.
  final Duration fallbackDuration;

  /// Extra time allowed on top of a clip's duration before playback is
  /// considered stuck and the sink moves on.
  final Duration watchdogGrace;

  final AudioPlayerFactory _playerFactory;
  final int _idSuffix;
  final List<_Slot> _slots = [];
  final List<Completer<void>> _idleWaiters = [];
  final Set<Completer<void>> _pending = {};

  _Slot? _playing;
  bool _stopped = false;
  bool _disposed = false;
  bool _disposeStarted = false;

  /// When the previous clip stopped sounding, used to report the handover gap.
  DateTime? _previousEndAt;

  static const int _slotCount = 2;

  @override
  Future<void> add(TtsAudioClip clip) async {
    if (_stopped || _disposed || clip.bytes.isEmpty) return;

    final finished = Completer<void>();
    _pending.add(finished);

    final slot = await _acquireIdleSlot();
    if (slot == null) {
      _retire(finished);
      return;
    }

    try {
      await slot.load(clip);
    } catch (e) {
      debugPrint('TTS: failed to buffer audio chunk: $e');
      _releaseSlot(slot);
      _retire(finished);
      return;
    }

    if (!slot.isReady || _stopped || _disposed) {
      _releaseSlot(slot);
      _retire(finished);
      return;
    }

    slot.pending = finished;
    if (_playing == null) {
      await _start(slot);
    }
  }

  @override
  Future<void> drain() async {
    while (_pending.isNotEmpty) {
      await _pending.first.future;
    }
  }

  @override
  Future<void> stop() async {
    _stopped = true;
    _playing = null;
    for (final slot in _slots) {
      slot.invalidate();
    }
    for (final slot in _slots) {
      try {
        await slot.player?.stop();
      } catch (_) {}
    }
    for (final completer in List<Completer<void>>.of(_pending)) {
      _retire(completer);
    }
    _notifyIdle();
  }

  @override
  Future<void> dispose() async {
    if (_disposeStarted) return;
    _disposeStarted = true;
    await stop();
    _disposed = true;
    for (final slot in _slots) {
      await slot.dispose();
    }
    _slots.clear();
  }

  Future<_Slot?> _acquireIdleSlot() async {
    while (!_stopped && !_disposed) {
      for (final slot in _slots) {
        if (slot.isIdle) return slot;
      }
      if (_slots.length < _slotCount) {
        final slot = _Slot(
          playerId: 'luna_tts_${_idSuffix}_${_slots.length}',
          factory: _playerFactory,
        );
        _slots.add(slot);
        return slot;
      }
      final waiter = Completer<void>();
      _idleWaiters.add(waiter);
      await waiter.future;
    }
    return null;
  }

  Future<void> _start(_Slot slot) async {
    final previousEnd = _previousEndAt;
    _previousEndAt = null;
    if (previousEnd != null) {
      // The audible silence between two chunks of one message.
      debugPrint('TTS: chunk handover gap '
          '${DateTime.now().difference(previousEnd).inMilliseconds}ms');
    }
    final token = slot.beginPlayback();
    _playing = slot;
    // Subscribe before resuming so a very short clip cannot complete first.
    slot.completionSubscription =
        slot.events.listen((_) => _onSlotDone(slot, token));
    slot.armWatchdog(_watchdogFor(slot), () => _onSlotDone(slot, token));
    try {
      await slot.player?.resume();
    } catch (e) {
      debugPrint('TTS: playback failed: $e');
      _onSlotDone(slot, token);
    }
  }

  Duration _watchdogFor(_Slot slot) {
    final reported = slot.duration;
    final base = (reported == null || reported <= Duration.zero)
        ? fallbackDuration
        : reported;
    return base + watchdogGrace;
  }

  void _onSlotDone(_Slot slot, int token) {
    if (slot.playToken != token || slot.state != _SlotState.playing) return;
    slot.cancelWatchdog();
    _previousEndAt = DateTime.now();
    if (_playing == slot) _playing = null;
    final pending = slot.pending;
    _releaseSlot(slot);
    _retire(pending);
    _notifyIdle();

    final next = _slots.firstWhere(
      (candidate) => candidate.state == _SlotState.ready,
      orElse: () => slot,
    );
    if (next != slot && !_stopped && !_disposed) {
      unawaited(_start(next));
    }
  }

  void _releaseSlot(_Slot slot) {
    slot.invalidate();
    _notifyIdle();
  }

  void _retire(Completer<void>? completer) {
    if (completer == null) return;
    _pending.remove(completer);
    if (!completer.isCompleted) completer.complete();
  }

  void _notifyIdle() {
    final waiters = List<Completer<void>>.of(_idleWaiters);
    _idleWaiters.clear();
    for (final waiter in waiters) {
      if (!waiter.isCompleted) waiter.complete();
    }
  }
}

enum _SlotState { idle, loading, ready, playing }

class _Slot {
  _Slot({required String playerId, required AudioPlayerFactory factory})
      : _playerId = playerId,
        _factory = factory;

  final String _playerId;
  final AudioPlayerFactory _factory;

  AudioPlayer? _player;
  StreamSubscription<void>? _completionSubscription;
  StreamSubscription<String>? _logSubscription;
  Timer? _watchdog;

  _SlotState state = _SlotState.idle;
  Completer<void>? pending;
  Duration? duration;
  int playToken = 0;

  bool get isIdle => state == _SlotState.idle;

  bool get isReady => state == _SlotState.ready;

  AudioPlayer? get player => _player;

  Stream<void> get events =>
      _player?.onPlayerComplete ?? const Stream<void>.empty();

  set completionSubscription(StreamSubscription<void>? subscription) {
    _completionSubscription = subscription;
  }

  AudioPlayer _ensurePlayer() {
    final existing = _player;
    if (existing != null) return existing;
    final player = _factory(_playerId);
    _player = player;
    _logSubscription = player.onLog.listen(
      (message) => debugPrint('TTS[$_playerId]: $message'),
    );
    return player;
  }

  Future<void> load(TtsAudioClip clip) async {
    final token = playToken;
    state = _SlotState.loading;
    duration = null;
    final player = _ensurePlayer();
    await player.setSourceBytes(clip.bytes, mimeType: clip.mimeType);
    final reported = await player.getDuration();
    // The slot may have been invalidated (stop/dispose) while preparing.
    if (playToken != token) return;
    duration = reported;
    state = _SlotState.ready;
  }

  int beginPlayback() {
    state = _SlotState.playing;
    return ++playToken;
  }

  /// Marks the slot free and invalidates any in-flight completion callback.
  void invalidate() {
    cancelWatchdog();
    state = _SlotState.idle;
    duration = null;
    pending = null;
    playToken++;
    final subscription = _completionSubscription;
    _completionSubscription = null;
    if (subscription != null) unawaited(subscription.cancel());
  }

  void armWatchdog(Duration timeout, void Function() onTimeout) {
    cancelWatchdog();
    _watchdog = Timer(timeout, () {
      debugPrint('TTS: playback watchdog fired after ${timeout.inMilliseconds}ms');
      onTimeout();
    });
  }

  void cancelWatchdog() {
    _watchdog?.cancel();
    _watchdog = null;
  }

  Future<void> dispose() async {
    invalidate();
    await _logSubscription?.cancel();
    _logSubscription = null;
    try {
      await _player?.dispose();
    } catch (_) {}
    _player = null;
  }
}
