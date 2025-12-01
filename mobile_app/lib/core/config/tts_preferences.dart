import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:shared_preferences/shared_preferences.dart';

class TtsPreferences {
  final bool enabled;
  final String language;

  const TtsPreferences({
    this.enabled = false,
    this.language = 'en-US',
  });

  factory TtsPreferences.defaults() {
    return const TtsPreferences(
      enabled: false,
      language: 'en-US',
    );
  }

  TtsPreferences copyWith({
    bool? enabled,
    String? language,
  }) {
    return TtsPreferences(
      enabled: enabled ?? this.enabled,
      language: language ?? this.language,
    );
  }
}

class TtsPreferencesNotifier extends StateNotifier<TtsPreferences> {
  TtsPreferencesNotifier() : super(TtsPreferences.defaults()) {
    _loadFuture = _loadFromPrefs();
  }

  static const _enabledKey = 'tts_enabled';
  static const _languageKey = 'tts_language';

  late final Future<void> _loadFuture;

  /// Wait for saved preferences to be loaded from SharedPreferences.
  Future<void> ensureLoaded() => _loadFuture;

  Future<void> _loadFromPrefs() async {
    final prefs = await SharedPreferences.getInstance();
    final enabled = prefs.getBool(_enabledKey) ?? false;
    final language = prefs.getString(_languageKey) ?? 'en-US';

    state = TtsPreferences(
      enabled: enabled,
      language: language,
    );
  }

  Future<void> _saveToPrefs() async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setBool(_enabledKey, state.enabled);
    await prefs.setString(_languageKey, state.language);
  }

  Future<void> setEnabled(bool value) async {
    state = state.copyWith(enabled: value);
    await _saveToPrefs();
  }

  Future<void> setLanguage(String value) async {
    state = state.copyWith(language: value);
    await _saveToPrefs();
  }
}

final ttsPreferencesProvider =
    StateNotifierProvider<TtsPreferencesNotifier, TtsPreferences>(
  (ref) => TtsPreferencesNotifier(),
);







