import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:shared_preferences/shared_preferences.dart';

class ServerConfig {
  final String host;
  final int port;
  final String apiKey;
  final String profile;

  const ServerConfig({
    required this.host,
    required this.port,
    required this.apiKey,
    required this.profile,
  });

  factory ServerConfig.defaults() {
    var host = '127.0.0.1';
    if (!kIsWeb && Platform.isAndroid) {
      host = '10.0.2.2';
    }
    return ServerConfig(
      host: host,
      port: 8080,
      apiKey: 'luna',
      profile: 'default',
    );
  }

  Uri websocketUri() => Uri(
        scheme: 'ws',
        host: host,
        port: port,
      );

  ServerConfig copyWith({
    String? host,
    int? port,
    String? apiKey,
    String? profile,
  }) {
    return ServerConfig(
      host: host ?? this.host,
      port: port ?? this.port,
      apiKey: apiKey ?? this.apiKey,
      profile: profile ?? this.profile,
    );
  }
}

class ServerConfigNotifier extends Notifier<ServerConfig> {
  static const _hostKey = 'server_host';
  static const _portKey = 'server_port';
  static const _apiKeyKey = 'server_api_key';
  static const _profileKey = 'server_profile';

  late final Future<void> _loadFuture;

  @override
  ServerConfig build() {
    _loadFuture = _loadFromPrefs();
    return ServerConfig.defaults();
  }

  /// Wait for saved config to be loaded from SharedPreferences.
  /// Call this before using config on startup.
  Future<void> ensureLoaded() => _loadFuture;

  Future<void> _loadFromPrefs() async {
    final prefs = await SharedPreferences.getInstance();
    final host = prefs.getString(_hostKey);
    final port = prefs.getInt(_portKey);
    final apiKey = prefs.getString(_apiKeyKey);
    final profile = prefs.getString(_profileKey);

    if (host != null || port != null || apiKey != null || profile != null) {
      state = ServerConfig(
        host: host ?? ServerConfig.defaults().host,
        port: port ?? ServerConfig.defaults().port,
        apiKey: apiKey ?? ServerConfig.defaults().apiKey,
        profile: profile ?? ServerConfig.defaults().profile,
      );
    }
  }

  Future<void> _saveToPrefs() async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString(_hostKey, state.host);
    await prefs.setInt(_portKey, state.port);
    await prefs.setString(_apiKeyKey, state.apiKey);
    await prefs.setString(_profileKey, state.profile);
  }

  void updateHost(String value) {
    state = state.copyWith(host: value.trim());
    _saveToPrefs();
  }

  void updatePort(int value) {
    state = state.copyWith(port: value);
    _saveToPrefs();
  }

  void updateApiKey(String value) {
    state = state.copyWith(apiKey: value.trim());
    _saveToPrefs();
  }

  void updateProfile(String value) {
    state = state.copyWith(profile: value.trim());
    _saveToPrefs();
  }
}

final serverConfigProvider =
    NotifierProvider<ServerConfigNotifier, ServerConfig>(
  ServerConfigNotifier.new,
);
