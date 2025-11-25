import 'dart:isolate';

import 'package:flutter_foreground_task/flutter_foreground_task.dart';

class ForegroundGuard {
  bool _initialized = false;
  bool _running = false;

  Future<void> init() async {
    if (_initialized) return;
    FlutterForegroundTask.init(
      foregroundTaskOptions: ForegroundTaskOptions(
        interval: 3000,
        allowWifiLock: true,
        autoRunOnBoot: false,
        isOnceEvent: false,
      ),
      androidNotificationOptions: AndroidNotificationOptions(
        channelId: 'luna_channel',
        channelName: 'Luna agent',
        channelDescription: 'Keeps the websocket alive during streaming.',
        channelImportance: NotificationChannelImportance.DEFAULT,
        priority: NotificationPriority.DEFAULT,
        iconData: NotificationIconData(
          resType: ResourceType.mipmap,
          resPrefix: ResourcePrefix.ic,
          name: 'launcher',
        ),
        buttons: [
          NotificationButton(id: 'stop', text: 'Stop'),
        ],
      ),
      iosNotificationOptions: IOSNotificationOptions(),
    );
    _initialized = true;
  }

  Future<void> ensureStarted(String summary) async {
    if (!_initialized) {
      await init();
    }
    if (_running) return;
    await FlutterForegroundTask.startService(
      notificationTitle: 'Luna is thinking…',
      notificationText: summary,
      callback: startLunaService,
    );
    _running = true;
  }

  Future<void> stop() async {
    if (_running) {
      await FlutterForegroundTask.stopService();
      _running = false;
    }
  }
}

@pragma('vm:entry-point')
void startLunaService() {
  FlutterForegroundTask.setTaskHandler(_LunaTaskHandler());
}

class _LunaTaskHandler extends TaskHandler {
  @override
  Future<void> onStart(DateTime timestamp, SendPort? sendPort) async {}

  @override
  Future<void> onEvent(DateTime timestamp, SendPort? sendPort) async {}

  @override
  Future<void> onDestroy(DateTime timestamp, SendPort? sendPort) async {}

  @override
  Future<void> onRepeatEvent(DateTime timestamp, SendPort? sendPort) async {}
}


