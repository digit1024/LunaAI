import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:luna_mobile/application/app_controller.dart';
import 'package:luna_mobile/main.dart';
import 'package:luna_mobile/services/foreground_guard.dart';
import 'package:luna_mobile/services/notification_service.dart';
import 'package:luna_mobile/ui/screens/connecting_screen.dart';

void main() {
  testWidgets('Shows connecting screen initially', (WidgetTester tester) async {
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          notificationServiceProvider.overrideWithValue(NotificationService()),
          foregroundGuardProvider.overrideWithValue(ForegroundGuard()),
        ],
        child: const LunaApp(),
      ),
    );

    expect(find.byType(ConnectingScreen), findsOneWidget);
  });
}
