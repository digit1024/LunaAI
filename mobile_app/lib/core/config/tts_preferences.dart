import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'tts_provider_type.dart';

class TtsPreferences {
  final bool enabled;
  final String language;
  final TtsProviderType providerType;

  const TtsPreferences({
    this.enabled = false,
    this.language = 'en-US',
    this.providerType = TtsProviderType.builtIn,
  });

  factory TtsPreferences.defaults() {
    return const TtsPreferences(
      enabled: false,
      language: 'en-US',
      providerType: TtsProviderType.builtIn,
    );
  }

  TtsPreferences copyWith({
    bool? enabled,
    String? language,
    TtsProviderType? providerType,
  }) {
    return TtsPreferences(
      enabled: enabled ?? this.enabled,
      language: language ?? this.language,
      providerType: providerType ?? this.providerType,
    );
  }
}

class TtsPreferencesNotifier extends Notifier<TtsPreferences> {
  static const _enabledKey = 'tts_enabled';
  static const _languageKey = 'tts_language';
  static const _providerTypeKey = 'tts_provider_type';

  late final Future<void> _loadFuture;

  @override
  TtsPreferences build() {
    _loadFuture = _loadFromPrefs();
    return TtsPreferences.defaults();
  }

  /// Wait for saved preferences to be loaded from SharedPreferences.
  Future<void> ensureLoaded() => _loadFuture;

  Future<void> _loadFromPrefs() async {
    final prefs = await SharedPreferences.getInstance();
    final enabled = prefs.getBool(_enabledKey) ?? false;
    final language = prefs.getString(_languageKey) ?? 'en-US';
    final providerTypeRaw = prefs.getString(_providerTypeKey);
    final providerType = providerTypeRaw != null
        ? (TtsProviderType.values
                .where((e) => e.name == providerTypeRaw)
                .firstOrNull ??
            TtsProviderType.builtIn)
        : TtsProviderType.builtIn;

    state = TtsPreferences(
      enabled: enabled,
      language: language,
      providerType: providerType,
    );
  }

  Future<void> _saveToPrefs() async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setBool(_enabledKey, state.enabled);
    await prefs.setString(_languageKey, state.language);
    await prefs.setString(_providerTypeKey, state.providerType.name);
  }

  Future<void> setEnabled(bool value) async {
    state = state.copyWith(enabled: value);
    await _saveToPrefs();
  }

  Future<void> setLanguage(String value) async {
    state = state.copyWith(language: value);
    await _saveToPrefs();
  }

  Future<void> setProviderType(TtsProviderType value) async {
    state = state.copyWith(providerType: value);
    await _saveToPrefs();
  }
}

final ttsPreferencesProvider =
    NotifierProvider<TtsPreferencesNotifier, TtsPreferences>(
  TtsPreferencesNotifier.new,
);









