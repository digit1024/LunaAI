import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_tts/flutter_tts.dart';
import '../utils/platform_utils.dart';

final ttsServiceProvider = Provider<TtsService>((ref) {
  final service = TtsService();
  return service;
});

class TtsService {
  final FlutterTts _flutterTts = FlutterTts();
  bool _initialized = false;
  VoidCallback? _onComplete;

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
    
    // Set completion handler
    _flutterTts.setCompletionHandler(() {
      _onComplete?.call();
      _onComplete = null;
    });
  }

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

  Future<void> speak(String text, {VoidCallback? onComplete}) async {
    if (text.trim().isEmpty) return;
    
    // Desktop/web: TTS disabled - call completion callback immediately
    if (!isMobile) {
      debugPrint('TTS: Disabled on desktop/web platform');
      onComplete?.call();
      return;
    }
    
    await init();
    _onComplete = onComplete;
    await _flutterTts.stop(); // Stop any ongoing speech
    await _flutterTts.speak(text);
  }

  Future<void> stop() async {
    if (!isMobile) return; // Desktop: No-op
    await init();
    await _flutterTts.stop();
  }

  void dispose() {
    if (!isMobile) return; // Desktop: No-op
    _flutterTts.stop();
  }
}

