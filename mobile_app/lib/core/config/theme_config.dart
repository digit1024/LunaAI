import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:shared_preferences/shared_preferences.dart';

/// Theme mode preference: system, light, or dark
enum ThemePreference { system, light, dark }

class ThemeConfig {
  final ThemePreference preference;

  const ThemeConfig({required this.preference});

  factory ThemeConfig.defaults() => const ThemeConfig(
        preference: ThemePreference.system,
      );

  ThemeMode get themeMode {
    switch (preference) {
      case ThemePreference.light:
        return ThemeMode.light;
      case ThemePreference.dark:
        return ThemeMode.dark;
      case ThemePreference.system:
        return ThemeMode.system;
    }
  }

  ThemeConfig copyWith({ThemePreference? preference}) {
    return ThemeConfig(preference: preference ?? this.preference);
  }
}

class ThemeConfigNotifier extends StateNotifier<ThemeConfig> {
  ThemeConfigNotifier() : super(ThemeConfig.defaults()) {
    _loadFuture = _loadFromPrefs();
  }

  static const _themeKey = 'theme_preference';

  late final Future<void> _loadFuture;

  Future<void> ensureLoaded() => _loadFuture;

  Future<void> _loadFromPrefs() async {
    final prefs = await SharedPreferences.getInstance();
    final themeStr = prefs.getString(_themeKey);

    if (themeStr != null) {
      final preference = ThemePreference.values.firstWhere(
        (e) => e.name == themeStr,
        orElse: () => ThemePreference.system,
      );
      state = ThemeConfig(preference: preference);
    }
  }

  Future<void> _saveToPrefs() async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString(_themeKey, state.preference.name);
  }

  void setTheme(ThemePreference preference) {
    state = state.copyWith(preference: preference);
    _saveToPrefs();
  }

  void toggleDarkMode() {
    final next = switch (state.preference) {
      ThemePreference.system => ThemePreference.dark,
      ThemePreference.dark => ThemePreference.light,
      ThemePreference.light => ThemePreference.system,
    };
    setTheme(next);
  }
}

final themeConfigProvider =
    StateNotifierProvider<ThemeConfigNotifier, ThemeConfig>(
  (ref) => ThemeConfigNotifier(),
);

