import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:shared_preferences/shared_preferences.dart';

/// Default Qween voice (qwen3-tts-flash supported).
const String kQweenDefaultVoice = 'Katerina';

/// Supported Qwen TTS voices for qwen3-tts-flash.
/// From https://www.alibabacloud.com/help/en/model-studio/qwen-tts#80027657a7cm4
const List<String> kQwenSupportedVoices = [
  'Cherry',
  'Serena',
  'Ethan',
  'Chelsie',
  'Momo',
  'Vivian',
  'Moon',
  'Maia',
  'Kai',
  'Nofish',
  'Bella',
  'Jennifer',
  'Ryan',
  'Katerina',
  'Aiden',
  'Eldric Sage',
  'Mia',
  'Mochi',
  'Bellona',
  'Vincent',
  'Bunny',
  'Neil',
  'Elias',
  'Arthur',
  'Nini',
  'Ebona',
  'Seren',
  'Pip',
  'Stella',
  'Bodega',
  'Sonrisa',
  'Alek',
  'Dolce',
  'Sohee',
  'Ono Anna',
  'Lenn',
  'Emilien',
  'Andre',
  'Radio Gol',
  // Dialect voices
  'Jada',
  'Dylan',
  'Li',
  'Marcus',
  'Roy',
  'Peter',
  'Sunny',
  'Eric',
  'Rocky',
  'Kiki',
];

class QweenTtsPreferences {
  final String voice;

  const QweenTtsPreferences({this.voice = kQweenDefaultVoice});

  QweenTtsPreferences copyWith({String? voice}) {
    return QweenTtsPreferences(voice: voice ?? this.voice);
  }
}

class QweenTtsPreferencesNotifier extends Notifier<QweenTtsPreferences> {
  static const _voiceKey = 'qween_tts_voice';
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
    state = QweenTtsPreferences(voice: voice);
  }

  Future<void> _saveToPrefs() async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString(_voiceKey, state.voice);
  }

  Future<void> setVoice(String value) async {
    state = state.copyWith(voice: value);
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
