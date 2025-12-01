import 'package:flutter_local_notifications/flutter_local_notifications.dart';

class NotificationService {
  final FlutterLocalNotificationsPlugin _plugin =
      FlutterLocalNotificationsPlugin();
  bool _initialized = false;

  Future<void> init() async {
    if (_initialized) return;
    const androidSettings =
        AndroidInitializationSettings('@mipmap/ic_launcher');
    const initializationSettings = InitializationSettings(
      android: androidSettings,
    );
    await _plugin.initialize(initializationSettings);
    _initialized = true;
  }

  Future<void> showResponseNotification({
    required String title,
    required String body,
  }) async {
    if (!_initialized) {
      await init();
    }
    const androidDetails = AndroidNotificationDetails(
      'luna_responses',
      'Luna responses',
      channelDescription: 'Notifications when Luna finishes a response.',
      importance: Importance.defaultImportance,
      priority: Priority.defaultPriority,
    );
    const notificationDetails = NotificationDetails(android: androidDetails);
    await _plugin.show(
      DateTime.now().millisecondsSinceEpoch ~/ 1000,
      title,
      body,
      notificationDetails,
    );
  }
}








