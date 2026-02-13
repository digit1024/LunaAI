import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:shared_preferences/shared_preferences.dart';

/// Default Qween TTS instructions per issue requirements.
const String kQweenDefaultInstructions =
    'speed: medium, Pitch Medium, Emotion: gentle and bit seductive, characteristics: Magnetic, Usage voice assistant colleague.';

/// Default Qween voice per supported system voices.
const String kQweenDefaultVoice = 'Katerina';

/// Supported Qwen TTS voices.
/// Cherry, Ethan, Chelsie, Vivian are confirmed in official docs/examples.
/// Katerina is specified in the issue requirements as default.
/// Others are from community reports of the 49-voice set.
const List<String> kQwenSupportedVoices = [
  'Katerina',
  'Cherry',
  'Ethan',
  'Chelsie',
  'Vivian',
];

class QweenTtsPreferences {
  final String voice;
  final String instructions;

  const QweenTtsPreferences({
    this.voice = kQweenDefaultVoice,
    this.instructions = kQweenDefaultInstructions,
  });

  QweenTtsPreferences copyWith({
    String? voice,
    String? instructions,
  }) {
    return QweenTtsPreferences(
      voice: voice ?? this.voice,
      instructions: instructions ?? this.instructions,
    );
  }
}

class QweenTtsPreferencesNotifier extends Notifier<QweenTtsPreferences> {
  static const _voiceKey = 'qween_tts_voice';
  static const _instructionsKey = 'qween_tts_instructions';
  static const _apiKeyStorageKey = 'qween_tts_api_key';

  final FlutterSecureStorage _secureStorage = const FlutterSecureStorage(
    aOptions: AndroidOptions(encryptedSharedPreferences: true),
  );

  late final Future<void> _loadFuture;

  @override
  QweenTtsPreferences build() {
    _loadFuture = _loadFromPrefs();
    return const QweenTtsPreferences();
  }

  Future<void> ensureLoaded() => _loadFuture;

  Future<void> _loadFromPrefs() async {
    final prefs = await SharedPreferences.getInstance();
    final voice = prefs.getString(_voiceKey) ?? kQweenDefaultVoice;
    final instructions =
        prefs.getString(_instructionsKey) ?? kQweenDefaultInstructions;

    state = QweenTtsPreferences(
      voice: voice,
      instructions: instructions,
    );
  }

  Future<void> _saveToPrefs() async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString(_voiceKey, state.voice);
    await prefs.setString(_instructionsKey, state.instructions);
  }

  Future<void> setVoice(String value) async {
    state = state.copyWith(voice: value);
    await _saveToPrefs();
  }

  Future<void> setInstructions(String value) async {
    state = state.copyWith(instructions: value);
    await _saveToPrefs();
  }

  /// Store API key in secure storage. Never log or expose.
  Future<void> setApiKey(String value) async {
    try {
      if (value.trim().isEmpty) {
        await _secureStorage.delete(key: _apiKeyStorageKey);
      } else {
        await _secureStorage.write(key: _apiKeyStorageKey, value: value.trim());
      }
    } catch (e) {
      debugPrint('QweenTts: Failed to store API key (secure storage error)');
      rethrow;
    }
  }

  /// Read API key from secure storage.
  Future<String?> getApiKey() async {
    try {
      return await _secureStorage.read(key: _apiKeyStorageKey);
    } catch (e) {
      debugPrint('QweenTts: Failed to read API key');
      return null;
    }
  }

  /// Whether API key is configured (non-empty).
  Future<bool> hasApiKey() async {
    final key = await getApiKey();
    return key != null && key.trim().isNotEmpty;
  }
}

final qweenTtsPreferencesProvider =
    NotifierProvider<QweenTtsPreferencesNotifier, QweenTtsPreferences>(
  QweenTtsPreferencesNotifier.new,
);
