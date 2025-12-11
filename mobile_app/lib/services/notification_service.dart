import 'dart:io';
import 'package:flutter/foundation.dart';
import 'package:flutter_local_notifications/flutter_local_notifications.dart';
import '../utils/platform_utils.dart';

class NotificationService {
  final FlutterLocalNotificationsPlugin _plugin =
      FlutterLocalNotificationsPlugin();
  bool _initialized = false;

  Future<void> init() async {
    if (_initialized) return;
    
    // Initialize based on platform
    if (Platform.isAndroid) {
      const androidSettings =
          AndroidInitializationSettings('@mipmap/ic_launcher');
      const initializationSettings = InitializationSettings(
        android: androidSettings,
      );
      await _plugin.initialize(initializationSettings);
      _initialized = true;
    } else if (Platform.isIOS) {
      // iOS initialization if needed
      const initializationSettings = InitializationSettings(
        iOS: DarwinInitializationSettings(),
      );
      await _plugin.initialize(initializationSettings);
      _initialized = true;
    } else if (isDesktop) {
      // Desktop: Try to initialize, but handle gracefully if not supported
      try {
        const initializationSettings = InitializationSettings();
        await _plugin.initialize(initializationSettings);
        _initialized = true;
      } catch (e) {
        debugPrint('Notifications not available on desktop: $e');
        _initialized = false;
      }
    } else {
      // Web or other platforms: Notifications not supported
      debugPrint('Notifications not available on this platform');
      _initialized = false;
    }
  }

  Future<void> showResponseNotification({
    required String title,
    required String body,
  }) async {
    if (!_initialized) {
      await init();
    }
    
    // Don't show notification if initialization failed (e.g., on desktop without support)
    if (!_initialized) {
      debugPrint('Cannot show notification: service not initialized');
      return;
    }
    
    // Platform-specific notification details
    NotificationDetails notificationDetails;
    if (Platform.isAndroid) {
      const androidDetails = AndroidNotificationDetails(
        'luna_responses',
        'Luna responses',
        channelDescription: 'Notifications when Luna finishes a response.',
        importance: Importance.defaultImportance,
        priority: Priority.defaultPriority,
      );
      notificationDetails = NotificationDetails(android: androidDetails);
    } else if (Platform.isIOS) {
      const iosDetails = DarwinNotificationDetails();
      notificationDetails = NotificationDetails(iOS: iosDetails);
    } else {
      // Desktop: Use default settings
      notificationDetails = const NotificationDetails();
    }
    
    try {
      await _plugin.show(
        DateTime.now().millisecondsSinceEpoch ~/ 1000,
        title,
        body,
        notificationDetails,
      );
    } catch (e) {
      debugPrint('Failed to show notification: $e');
    }
  }
}










