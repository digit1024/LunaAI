import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:web_socket_channel/io.dart';

import '../../core/config/server_config.dart';
import 'ws_dto.dart';

typedef EventHandler = void Function(ServerEvent event);

class LunaWsClient {
  LunaWsClient(this._config);

  ServerConfig _config;
  IOWebSocketChannel? _channel;
  StreamSubscription? _subscription;
  final _eventController = StreamController<ServerEvent>.broadcast();
  bool _disposed = false;

  Stream<ServerEvent> get events => _eventController.stream;

  Future<void> connect() async {
    await _disposeChannel();
    _disposed = false;
    await _openChannel();
  }

  Future<void> reconnectWith(ServerConfig config) async {
    _config = config;
    await connect();
  }

  Future<void> _openChannel() async {
    final uri = _config.websocketUri();
    final headers = {
      'x-api-key': _config.apiKey,
      'authorization': 'Bearer ${_config.apiKey}',
    };
    try {
      final channel = IOWebSocketChannel.connect(uri, headers: headers);
      _channel = channel;
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
          if (!_disposed) {
            _scheduleReconnect();
          }
        },
        onError: (_) {
          if (!_disposed) {
            _scheduleReconnect();
          }
        },
      );
    } catch (_) {
      if (!_disposed) {
        await _scheduleReconnect();
      }
    }
  }

  void send(ClientCommand command) {
    final sink = _channel?.sink;
    if (sink != null) {
      sink.add(jsonEncode(command.toJson()));
    }
  }

  Future<void> _scheduleReconnect() async {
    await Future<void>.delayed(const Duration(seconds: 2));
    if (!_disposed) {
      await _openChannel();
    }
  }

  Future<void> dispose() async {
    _disposed = true;
    await _disposeChannel();
    await _eventController.close();
  }

  Future<void> _disposeChannel() async {
    await _subscription?.cancel();
    await _channel?.sink.close(WebSocketStatus.normalClosure);
    _subscription = null;
    _channel = null;
  }
}

