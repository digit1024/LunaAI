import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'voice_catalog.dart';
import 'voice_models.dart';

class VoicePreferences {
  const VoicePreferences({
    this.ttsEnabled = false,
    this.inputMode = VoiceInputMode.tap,
    this.language = 'en-US',
    this.stt = const SttConfig(),
    this.tts = const TtsConfig(),
    this.vocabulary = const [],
  });

  final bool ttsEnabled;
  final VoiceInputMode inputMode;
  final String language;
  final SttConfig stt;
  final TtsConfig tts;
  final List<String> vocabulary;

  VoicePreferences copyWith({
    bool? ttsEnabled,
    VoiceInputMode? inputMode,
    String? language,
    SttConfig? stt,
    TtsConfig? tts,
    List<String>? vocabulary,
  }) {
    return VoicePreferences(
      ttsEnabled: ttsEnabled ?? this.ttsEnabled,
      inputMode: inputMode ?? this.inputMode,
      language: language ?? this.language,
      stt: stt ?? this.stt,
      tts: tts ?? this.tts,
      vocabulary: vocabulary ?? this.vocabulary,
    );
  }

  Map<String, dynamic> toJson() => {
        'ttsEnabled': ttsEnabled,
        'inputMode': inputMode.name,
        'language': language,
        'stt': stt.toJson(),
        'tts': tts.toJson(),
        'vocabulary': vocabulary,
      };

  factory VoicePreferences.fromJson(Map<String, dynamic> json) {
    VoiceInputMode mode = VoiceInputMode.tap;
    final rawMode = json['inputMode'] as String?;
    if (rawMode != null) {
      mode = VoiceInputMode.values.firstWhere(
        (m) => m.name == rawMode,
        orElse: () => VoiceInputMode.tap,
      );
    }
    return VoicePreferences(
      ttsEnabled: json['ttsEnabled'] as bool? ?? false,
      inputMode: mode,
      language: json['language'] as String? ?? 'en-US',
      stt: json['stt'] is Map<String, dynamic>
          ? SttConfig.fromJson(json['stt'] as Map<String, dynamic>)
          : const SttConfig(),
      tts: json['tts'] is Map<String, dynamic>
          ? TtsConfig.fromJson(json['tts'] as Map<String, dynamic>)
          : const TtsConfig(),
      vocabulary: (json['vocabulary'] as List<dynamic>? ?? const [])
          .map((e) => e.toString())
          .where((e) => e.trim().isNotEmpty)
          .toList(),
    );
  }
}

class VoicePreferencesNotifier extends Notifier<VoicePreferences> {
  static const _prefsKey = 'voice_prefs_v1';
  static const _legacyQwenApiKey = 'qween_tts_api_key';

  final FlutterSecureStorage _secureStorage = const FlutterSecureStorage(
    aOptions: AndroidOptions(encryptedSharedPreferences: true),
  );

  late final Future<void> _loadFuture;

  @override
  VoicePreferences build() {
    _loadFuture = _load();
    return const VoicePreferences();
  }

  Future<void> ensureLoaded() => _loadFuture;

  Future<void> _load() async {
    final prefs = await SharedPreferences.getInstance();
    final raw = prefs.getString(_prefsKey);
    if (raw != null && raw.isNotEmpty) {
      try {
        final json = jsonDecode(raw) as Map<String, dynamic>;
        state = VoicePreferences.fromJson(json);
        return;
      } catch (e) {
        debugPrint('VoicePreferences: failed to parse saved prefs: $e');
      }
    }
    state = await _migrateLegacy(prefs);
    await _save();
  }

  Future<VoicePreferences> _migrateLegacy(SharedPreferences prefs) async {
    final enabled = prefs.getBool('tts_enabled') ?? false;
    final language = prefs.getString('tts_language') ??
        prefs.getString('stt_language') ??
        'en-US';
    final legacyProvider = prefs.getString('tts_provider_type');
    final legacyVoice = prefs.getString('qween_tts_voice');

    var tts = const TtsConfig();
    if (legacyProvider == 'qween') {
      try {
        final key = await _secureStorage.read(key: _legacyQwenApiKey);
        if (key != null && key.trim().isNotEmpty) {
          await _secureStorage.write(
            key: _apiKeyStorageKey('qwen'),
            value: key.trim(),
          );
        }
      } catch (e) {
        debugPrint('VoicePreferences: legacy Qwen key migration failed');
      }
      tts = TtsConfig(
        providerId: 'qwen',
        modelId: 'qwen3-tts-flash',
        voice: legacyVoice ?? 'Katerina',
      );
    }

    return VoicePreferences(
      ttsEnabled: enabled,
      inputMode: VoiceInputMode.tap,
      language: language,
      stt: const SttConfig(),
      tts: tts,
      vocabulary: const [],
    );
  }

  Future<void> _save() async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString(_prefsKey, jsonEncode(state.toJson()));
  }

  static String _apiKeyStorageKey(String providerId) =>
      'voice_api_key_$providerId';

  Future<void> setTtsEnabled(bool value) async {
    state = state.copyWith(ttsEnabled: value);
    await _save();
  }

  Future<void> setInputMode(VoiceInputMode value) async {
    state = state.copyWith(inputMode: value);
    await _save();
  }

  Future<void> setLanguage(String value) async {
    state = state.copyWith(language: value);
    await _save();
  }

  Future<void> setSttProvider(String providerId) async {
    final models = VoiceCatalog.sttModelsFor(providerId);
    final model = models.isNotEmpty ? models.first : null;
    state = state.copyWith(
      stt: state.stt.copyWith(
        providerId: providerId,
        modelId: model?.id ?? 'custom',
      ),
    );
    await _save();
  }

  Future<void> setSttModel(String modelId) async {
    state = state.copyWith(stt: state.stt.copyWith(modelId: modelId));
    await _save();
  }

  Future<void> setSttEndpointOverride(String value) async {
    state = state.copyWith(
      stt: state.stt.copyWith(endpointOverride: value.trim()),
    );
    await _save();
  }

  Future<void> setSttModelOverride(String value) async {
    state = state.copyWith(
      stt: state.stt.copyWith(modelOverride: value.trim()),
    );
    await _save();
  }

  Future<void> setSttExtraParams(String value) async {
    state = state.copyWith(
      stt: state.stt.copyWith(extraParamsJson: value.trim()),
    );
    await _save();
  }

  Future<void> setTtsProvider(String providerId) async {
    final models = VoiceCatalog.ttsModelsFor(providerId);
    final model = models.isNotEmpty ? models.first : null;
    state = state.copyWith(
      tts: state.tts.copyWith(
        providerId: providerId,
        modelId: model?.id ?? 'custom',
        voice: model?.defaultVoice ?? '',
      ),
    );
    await _save();
  }

  Future<void> setTtsModel(String modelId) async {
    final model = VoiceCatalog.ttsModel(state.tts.providerId, modelId);
    state = state.copyWith(
      tts: state.tts.copyWith(
        modelId: modelId,
        voice: model.defaultVoice,
      ),
    );
    await _save();
  }

  Future<void> setTtsVoice(String voice) async {
    state = state.copyWith(tts: state.tts.copyWith(voice: voice));
    await _save();
  }

  Future<void> setTtsEndpointOverride(String value) async {
    state = state.copyWith(
      tts: state.tts.copyWith(endpointOverride: value.trim()),
    );
    await _save();
  }

  Future<void> setTtsModelOverride(String value) async {
    state = state.copyWith(
      tts: state.tts.copyWith(modelOverride: value.trim()),
    );
    await _save();
  }

  Future<void> setTtsExtraParams(String value) async {
    state = state.copyWith(
      tts: state.tts.copyWith(extraParamsJson: value.trim()),
    );
    await _save();
  }

  Future<void> addVocabularyTerm(String term) async {
    final trimmed = term.trim();
    if (trimmed.isEmpty || state.vocabulary.contains(trimmed)) return;
    state = state.copyWith(vocabulary: [...state.vocabulary, trimmed]);
    await _save();
  }

  Future<void> removeVocabularyTerm(String term) async {
    state = state.copyWith(
      vocabulary: state.vocabulary.where((t) => t != term).toList(),
    );
    await _save();
  }

  Future<void> setApiKey(String providerId, String value) async {
    try {
      final key = _apiKeyStorageKey(providerId);
      if (value.trim().isEmpty) {
        await _secureStorage.delete(key: key);
      } else {
        await _secureStorage.write(key: key, value: value.trim());
      }
    } catch (e) {
      debugPrint('VoicePreferences: failed to store API key');
      rethrow;
    }
  }

  Future<String?> getApiKey(String providerId) async {
    try {
      return await _secureStorage.read(key: _apiKeyStorageKey(providerId));
    } catch (e) {
      debugPrint('VoicePreferences: failed to read API key');
      return null;
    }
  }

  static String languageToIso(String language) {
    final parts = language.split(RegExp(r'[-_]'));
    return parts.first.toLowerCase();
  }

  static String languageToQwenName(String language) {
    final iso = languageToIso(language);
    const names = {
      'zh': 'Chinese',
      'en': 'English',
      'de': 'German',
      'it': 'Italian',
      'pt': 'Portuguese',
      'es': 'Spanish',
      'ja': 'Japanese',
      'ko': 'Korean',
      'fr': 'French',
      'ru': 'Russian',
    };
    return names[iso] ?? 'Auto';
  }
}

final voicePreferencesProvider =
    NotifierProvider<VoicePreferencesNotifier, VoicePreferences>(
  VoicePreferencesNotifier.new,
);
