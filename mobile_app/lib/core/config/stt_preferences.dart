import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:shared_preferences/shared_preferences.dart';

class SttPreferences {
  final bool enabled;
  final String language;
  final Duration pauseDuration; // Duration to wait before sending STT result
  final List<String> favoriteLanguages; // List of favorite language codes

  const SttPreferences({
    this.enabled = true,
    this.language = 'en-US',
    Duration? pauseDuration,
    List<String>? favoriteLanguages,
  }) : pauseDuration = pauseDuration ?? const Duration(seconds: 2),
       favoriteLanguages = favoriteLanguages ?? const ['en-US'];

  factory SttPreferences.defaults() {
    return const SttPreferences(
      enabled: true,
      language: 'en-US',
      pauseDuration: Duration(seconds: 2),
      favoriteLanguages: ['en-US'],
    );
  }

  SttPreferences copyWith({
    bool? enabled,
    String? language,
    Duration? pauseDuration,
    List<String>? favoriteLanguages,
  }) {
    return SttPreferences(
      enabled: enabled ?? this.enabled,
      language: language ?? this.language,
      pauseDuration: pauseDuration ?? this.pauseDuration,
      favoriteLanguages: favoriteLanguages ?? this.favoriteLanguages,
    );
  }
}

class SttPreferencesNotifier extends Notifier<SttPreferences> {
  static const _enabledKey = 'stt_enabled';
  static const _languageKey = 'stt_language';
  static const _pauseDurationKey = 'stt_pause_duration_seconds';
  static const _favoriteLanguagesKey = 'stt_favorite_languages';

  late final Future<void> _loadFuture;

  @override
  SttPreferences build() {
    _loadFuture = _loadFromPrefs();
    return SttPreferences.defaults();
  }

  /// Wait for saved preferences to be loaded from SharedPreferences.
  Future<void> ensureLoaded() => _loadFuture;

  Future<void> _loadFromPrefs() async {
    final prefs = await SharedPreferences.getInstance();
    final enabled = prefs.getBool(_enabledKey) ?? true;
    final language = prefs.getString(_languageKey) ?? 'en-US';
    final pauseSeconds = prefs.getInt(_pauseDurationKey) ?? 2;
    final favoriteLanguagesList = prefs.getStringList(_favoriteLanguagesKey) ?? ['en-US'];

    state = SttPreferences(
      enabled: enabled,
      language: language,
      pauseDuration: Duration(seconds: pauseSeconds),
      favoriteLanguages: favoriteLanguagesList,
    );
  }

  Future<void> _saveToPrefs() async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setBool(_enabledKey, state.enabled);
    await prefs.setString(_languageKey, state.language);
    await prefs.setInt(_pauseDurationKey, state.pauseDuration.inSeconds);
    await prefs.setStringList(_favoriteLanguagesKey, state.favoriteLanguages);
  }

  Future<void> setEnabled(bool value) async {
    state = state.copyWith(enabled: value);
    await _saveToPrefs();
  }

  Future<void> setLanguage(String value) async {
    state = state.copyWith(language: value);
    // Automatically add to favorites if not already there
    if (!state.favoriteLanguages.contains(value)) {
      state = state.copyWith(
        favoriteLanguages: [...state.favoriteLanguages, value],
      );
    }
    await _saveToPrefs();
  }

  Future<void> setPauseDuration(Duration duration) async {
    state = state.copyWith(pauseDuration: duration);
    await _saveToPrefs();
  }

  Future<void> addFavoriteLanguage(String languageCode) async {
    if (!state.favoriteLanguages.contains(languageCode)) {
      state = state.copyWith(
        favoriteLanguages: [...state.favoriteLanguages, languageCode],
      );
      await _saveToPrefs();
    }
  }

  Future<void> removeFavoriteLanguage(String languageCode) async {
    if (state.favoriteLanguages.length > 1) {
      // Don't allow removing the last favorite
      state = state.copyWith(
        favoriteLanguages: state.favoriteLanguages
            .where((lang) => lang != languageCode)
            .toList(),
      );
      await _saveToPrefs();
    }
  }

  Future<void> setFavoriteLanguages(List<String> languages) async {
    if (languages.isNotEmpty) {
      state = state.copyWith(favoriteLanguages: languages);
      await _saveToPrefs();
    }
  }
}

final sttPreferencesProvider =
    NotifierProvider<SttPreferencesNotifier, SttPreferences>(
  SttPreferencesNotifier.new,
);








