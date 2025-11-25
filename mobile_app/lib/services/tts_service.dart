import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_tts/flutter_tts.dart';

final ttsServiceProvider = Provider<TtsService>((ref) {
  final service = TtsService();
  return service;
});

class TtsService {
  final FlutterTts _flutterTts = FlutterTts();
  bool _initialized = false;

  Future<void> init() async {
    if (_initialized) return;
    _initialized = true;
    
    // Set default settings
    await _flutterTts.setLanguage('en-US');
    await _flutterTts.setSpeechRate(0.5); // Slower speech rate
    await _flutterTts.setVolume(1.0);
    await _flutterTts.setPitch(1.0);
  }

  Future<void> setLanguage(String languageCode) async {
    await init();
    await _flutterTts.setLanguage(languageCode);
  }

  Future<List<dynamic>> getLanguages() async {
    await init();
    final languages = await _flutterTts.getLanguages;
    return languages ?? [];
  }

  Future<void> speak(String text) async {
    if (text.trim().isEmpty) return;
    await init();
    await _flutterTts.stop(); // Stop any ongoing speech
    await _flutterTts.speak(text);
  }

  Future<void> stop() async {
    await init();
    await _flutterTts.stop();
  }

  void dispose() {
    _flutterTts.stop();
  }
}

