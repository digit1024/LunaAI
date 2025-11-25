import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:web_socket_channel/io.dart';

import '../../core/config/server_config.dart';
import 'ws_dto.dart';

typedef EventHandler = void Function(ServerEvent event);

/// Singleton WebSocket client for Luna server communication.
/// 
/// Connection retries are handled by the caller (AppController).
/// This class does NOT auto-reconnect to prevent connection storms.
class LunaWsClient {
  LunaWsClient();

  static const Duration _connectionTimeout = Duration(seconds: 5);

  ServerConfig? _config;
  IOWebSocketChannel? _channel;
  StreamSubscription? _subscription;
  final _eventController = StreamController<ServerEvent>.broadcast();
  bool _disposed = false;

  Stream<ServerEvent> get events => _eventController.stream;
  bool get isConnected => _channel != null && !_disposed;

  /// Connect to the server with the given config.
  /// Throws [Exception] if connection fails - caller should handle retries.
  Future<void> connect(ServerConfig config) async {
    _config = config;
    await _disposeChannel();
    _disposed = false;
    await _openChannel();
  }

  Future<void> _openChannel() async {
    final config = _config;
    if (config == null) {
      throw Exception('No server config provided');
    }

    final uri = config.websocketUri();
    final headers = {
      'x-api-key': config.apiKey,
      'authorization': 'Bearer ${config.apiKey}',
    };

    try {
      debugPrint('🔌 Connecting to $uri');
      final channel = IOWebSocketChannel.connect(uri, headers: headers);
      _channel = channel;

      // Wait for connection with timeout
      await channel.ready.timeout(
        _connectionTimeout,
        onTimeout: () => throw TimeoutException('Connection timed out', _connectionTimeout),
      );
      debugPrint('✅ WebSocket connected');

      _subscription = channel.stream.listen(
        (raw) {
          try {
            final decoded = jsonDecode(raw as String) as Map<String, dynamic>;
            final event = ServerEvent.fromJson(decoded);
            _eventController.add(event);
          } catch (err) {
            _eventController.add(UnknownEvent(err.toString()));
          }
        },
        onDone: () {
          debugPrint('🔌 WebSocket closed');
          _channel = null;
          // Emit disconnect event so UI can handle it
          if (!_disposed) {
            _eventController.add(DisconnectedEvent());
          }
        },
        onError: (err) {
          debugPrint('❌ WebSocket error: $err');
          if (!_disposed) {
            _eventController.add(DisconnectedEvent());
          }
        },
      );
    } catch (e) {
      debugPrint('❌ Connection failed: $e');
      _channel = null;
      rethrow; // Let caller handle retry logic
    }
  }

  void send(ClientCommand command) {
    final sink = _channel?.sink;
    if (sink != null) {
      sink.add(jsonEncode(command.toJson()));
    } else {
      debugPrint('⚠️ Cannot send - not connected');
    }
  }

  Future<void> dispose() async {
    _disposed = true;
    await _disposeChannel();
    await _eventController.close();
  }

  Future<void> _disposeChannel() async {
    await _subscription?.cancel();
    try {
      await _channel?.sink.close(WebSocketStatus.normalClosure);
    } catch (_) {
      // Ignore close errors
    }
    _subscription = null;
    _channel = null;
  }
}

