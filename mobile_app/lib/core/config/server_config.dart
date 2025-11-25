import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

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

class ServerConfigNotifier extends StateNotifier<ServerConfig> {
  ServerConfigNotifier() : super(ServerConfig.defaults());

  void updateHost(String value) {
    state = state.copyWith(host: value.trim());
  }

  void updatePort(int value) {
    state = state.copyWith(port: value);
  }

  void updateApiKey(String value) {
    state = state.copyWith(apiKey: value.trim());
  }

  void updateProfile(String value) {
    state = state.copyWith(profile: value.trim());
  }
}

final serverConfigProvider =
    StateNotifierProvider<ServerConfigNotifier, ServerConfig>(
  (ref) => ServerConfigNotifier(),
);
