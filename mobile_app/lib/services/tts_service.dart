import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_tts/flutter_tts.dart';
import '../utils/platform_utils.dart';
import 'device_tts_engine.dart';

final ttsServiceProvider = Provider<TtsService>((ref) {
  final service = TtsService();
  return service;
});

class TtsService implements DeviceTtsEngine {
  final FlutterTts _flutterTts = FlutterTts();

  /// One callback per utterance handed to [speak], in submission order.
  final List<VoidCallback> _utteranceCallbacks = [];

  bool _initialized = false;
  bool _queueing = false;

  /// `TextToSpeech.QUEUE_ADD`, i.e. append to what is already speaking.
  static const int _queueAdd = 1;

  /// `TextToSpeech.QUEUE_FLUSH`, i.e. interrupt and replace.
  static const int _queueFlush = 0;

  @override
  bool get supportsUtteranceQueue => isAndroid;

  Future<void> init() async {
    if (_initialized) return;
    _initialized = true;

    // TTS only available on mobile platforms
    if (!isMobile) {
      debugPrint('TTS: Not available on desktop/web platform');
      return;
    }

    // Set default settings
    await _flutterTts.setLanguage('en-US');
    await _flutterTts.setSpeechRate(0.5); // Slower speech rate
    await _flutterTts.setVolume(1.0);
    await _flutterTts.setPitch(1.0);

    // The engine reports the end of an utterance through one of three
    // handlers; each means "no further event will arrive for it".
    _flutterTts.setCompletionHandler(_fireNextUtterance);
    _flutterTts.setCancelHandler(_fireNextUtterance);
    _flutterTts.setErrorHandler((message) {
      debugPrint('TTS: engine error: $message');
      _fireNextUtterance();
    });
  }

  @override
  Future<void> setLanguage(String languageCode) async {
    if (!isMobile) return; // Desktop: No-op
    await init();
    await _flutterTts.setLanguage(languageCode);
  }

  Future<List<dynamic>> getLanguages() async {
    if (!isMobile) return []; // Desktop: Return empty list
    await init();
    final languages = await _flutterTts.getLanguages;
    return languages ?? [];
  }

  @override
  Future<void> setUtteranceQueueing(bool enabled) async {
    if (!supportsUtteranceQueue || _queueing == enabled) return;
    await init();
    _queueing = enabled;
    await _flutterTts.setQueueMode(enabled ? _queueAdd : _queueFlush);
  }

  @override
  Future<void> speak(String text, {VoidCallback? onComplete}) async {
    if (text.trim().isEmpty) {
      onComplete?.call();
      return;
    }

    // Desktop/web: TTS disabled - call completion callback immediately
    if (!isMobile) {
      debugPrint('TTS: Disabled on desktop/web platform');
      onComplete?.call();
      return;
    }

    await init();
    // While queueing, stopping here would flush the utterances already
    // appended for this message. Otherwise stop first: it releases the
    // callback of whatever was interrupted, and registering ours afterwards
    // keeps the cancel event from firing it early.
    if (!_queueing) await _flutterTts.stop();
    if (onComplete != null) _utteranceCallbacks.add(onComplete);
    await _flutterTts.speak(text);
  }

  @override
  Future<void> stop() async {
    if (!isMobile) return; // Desktop: No-op
    await init();
    // Drop callbacks before stopping: the engine reports a single cancellation
    // for the utterance in flight and nothing for the ones still queued, so
    // forwarding its events would desynchronise the queue. Callers own
    // completion reporting for an interrupted message.
    _utteranceCallbacks.clear();
    if (_queueing) await setUtteranceQueueing(false);
    await _flutterTts.stop();
  }

  void dispose() {
    if (!isMobile) return; // Desktop: No-op
    _utteranceCallbacks.clear();
    _flutterTts.stop();
  }

  void _fireNextUtterance() {
    if (_utteranceCallbacks.isEmpty) return;
    _utteranceCallbacks.removeAt(0)();
  }
}
