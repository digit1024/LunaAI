import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:luna_mobile/main.dart';
import 'package:luna_mobile/ui/screens/setup_screen.dart';

// This is a basic Flutter widget test.
//
// To perform an interaction with a widget in your test, use the WidgetTester
// utility in the flutter_test package. For example, you can send tap and scroll
// gestures. You can also use WidgetTester to find child widgets in the widget
// tree, read text, and verify that the values of widget properties are correct.
void main() {
  testWidgets('Shows setup screen initially', (WidgetTester tester) async {
    // Build our app and trigger a frame.
    await tester.pumpWidget(ProviderScope(
      overrides: [
        // Mock the providers that would normally be overridden in main()
        // For simplicity in testing, we'll just test the basic routing
      ],
      child: const LunaApp(),
    ));

    // Verify that the setup screen is shown.
    expect(find.byType(SetupScreen), findsOneWidget);
  });
}
