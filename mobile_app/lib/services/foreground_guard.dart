import 'package:flutter/foundation.dart';
import 'package:flutter_foreground_task/flutter_foreground_task.dart';
import '../utils/platform_utils.dart';

class ForegroundGuard {
  bool _initialized = false;
  bool _running = false;
  bool _connectionGuardActive = false;

  Future<void> init() async {
    if (_initialized) return;
    
    // Only initialize foreground service on mobile platforms
    // Desktop apps don't need foreground services - connection stays alive naturally
    if (isMobile) {
      FlutterForegroundTask.init(
        foregroundTaskOptions: ForegroundTaskOptions(
          eventAction: ForegroundTaskEventAction.nothing(),
          autoRunOnBoot: false,
          allowWakeLock: true,
          allowWifiLock: true,
        ),
        androidNotificationOptions: AndroidNotificationOptions(
          channelId: 'luna_channel',
          channelName: 'Luna agent',
          channelDescription: 'Keeps the websocket alive during streaming.',
          channelImportance: NotificationChannelImportance.DEFAULT,
          priority: NotificationPriority.DEFAULT,
        ),
        iosNotificationOptions: const IOSNotificationOptions(),
      );
    }
    // Desktop: No-op - connection managed by app lifecycle
    _initialized = true;
  }

  /// Start service for streaming (temporary, but connection guard keeps it running)
  Future<void> ensureStarted(String summary) async {
    if (!_initialized) {
      await init();
    }
    
    // Desktop: No-op - connection stays alive naturally
    if (!isMobile) {
      return;
    }
    
    // If service is already running (connection guard), keep it running
    // The notification will show "Connected to server" which is fine
    if (_running) {
      // Service already running via connection guard, no need to restart
      return;
    }
    await FlutterForegroundTask.startService(
      notificationTitle: 'Luna is thinking…',
      notificationText: summary,
      callback: startLunaService,
    );
    _running = true;
  }

  /// Start service to keep connection alive (continuous, until explicitly stopped)
  Future<void> startConnectionGuard() async {
    if (!_initialized) {
      await init();
    }
    
    // Desktop: No-op - connection managed by app lifecycle
    if (!isMobile) {
      return;
    }
    
    if (_connectionGuardActive) return;
    _connectionGuardActive = true;
    if (!_running) {
      await FlutterForegroundTask.startService(
        notificationTitle: 'Luna',
        notificationText: 'Connected to server',
        callback: startLunaService,
      );
      _running = true;
    }
  }

  /// Stop connection guard (but keep service if streaming is active)
  Future<void> stopConnectionGuard() async {
    _connectionGuardActive = false;
    // Don't stop service here - let it continue if needed for streaming
    // Service will be stopped explicitly on disconnect
    // Notification will remain as is (either "Connected to server" or "Luna is thinking...")
  }

  Future<void> stop() async {
    // Desktop: No-op
    if (!isMobile) {
      return;
    }
    
    if (_running) {
      await FlutterForegroundTask.stopService();
      _running = false;
      _connectionGuardActive = false;
    }
  }
}

@pragma('vm:entry-point')
void startLunaService() {
  FlutterForegroundTask.setTaskHandler(_LunaTaskHandler());
}

class _LunaTaskHandler extends TaskHandler {
  @override
  Future<void> onStart(DateTime timestamp, TaskStarter starter) async {}

  @override
  void onRepeatEvent(DateTime timestamp) {}

  @override
  Future<void> onDestroy(DateTime timestamp, bool isTimeout) async {}
}


