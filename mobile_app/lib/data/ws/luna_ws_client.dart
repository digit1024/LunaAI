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
  Timer? _pingTimer;
  DateTime? _lastPongReceived;

  static const Duration _pingInterval = Duration(seconds: 30);
  static const Duration _pongTimeout = Duration(seconds: 60);

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

      _lastPongReceived = DateTime.now();
      _startPingTimer();

      _subscription = channel.stream.listen(
        (raw) {
          try {
            // Update last pong time (any message indicates connection is alive)
            _lastPongReceived = DateTime.now();
            
            final decoded = jsonDecode(raw as String) as Map<String, dynamic>;
            final event = ServerEvent.fromJson(decoded);
            _eventController.add(event);
          } catch (err) {
            _eventController.add(UnknownEvent(err.toString()));
          }
        },
        onDone: () {
          debugPrint('🔌 WebSocket closed');
          _stopPingTimer();
          _channel = null;
          // Emit disconnect event so UI can handle it
          if (!_disposed) {
            _eventController.add(DisconnectedEvent());
          }
        },
        onError: (err) {
          debugPrint('❌ WebSocket error: $err');
          _stopPingTimer();
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
    _stopPingTimer();
    await _disposeChannel();
    await _eventController.close();
  }

  Future<void> _disposeChannel() async {
    _stopPingTimer();
    await _subscription?.cancel();
    try {
      await _channel?.sink.close(WebSocketStatus.normalClosure);
    } catch (_) {
      // Ignore close errors
    }
    _subscription = null;
    _channel = null;
    _lastPongReceived = null;
  }

  void _startPingTimer() {
    _stopPingTimer();
    _pingTimer = Timer.periodic(_pingInterval, (timer) {
      if (_disposed || _channel == null) {
        timer.cancel();
        return;
      }

      // Check if we've received a pong recently
      if (_lastPongReceived != null) {
        final timeSinceLastPong = DateTime.now().difference(_lastPongReceived!);
        if (timeSinceLastPong > _pongTimeout) {
          debugPrint('⚠️ No pong received for ${timeSinceLastPong.inSeconds}s, connection may be dead');
          // Connection appears dead, emit disconnect
          if (!_disposed) {
            _eventController.add(DisconnectedEvent());
          }
          timer.cancel();
          return;
        }
      }

      // Send ping frame
      try {
        final socket = _channel?.sink;
        if (socket != null) {
          // IOWebSocketChannel doesn't expose ping directly, but the underlying
          // WebSocket will handle ping/pong automatically via the protocol
          // We'll use a health check command instead as a keepalive
          // The server responds to health checks, which serves as our "pong"
          send(ClientCommand.healthCheck());
          debugPrint('💓 Ping sent (health check)');
        }
      } catch (e) {
        debugPrint('❌ Failed to send ping: $e');
        timer.cancel();
        if (!_disposed) {
          _eventController.add(DisconnectedEvent());
        }
      }
    });
  }

  void _stopPingTimer() {
    _pingTimer?.cancel();
    _pingTimer = null;
  }
}

