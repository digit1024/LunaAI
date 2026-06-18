import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'application/app_controller.dart';
import 'application/app_state.dart';
import 'core/config/server_config.dart';
import 'core/config/theme_config.dart';
import 'core/theme/app_theme.dart';
import 'services/foreground_guard.dart';
import 'services/notification_service.dart';
import 'ui/screens/chat_screen.dart';
import 'ui/screens/connecting_screen.dart';
import 'ui/screens/conversations_screen.dart';
import 'ui/screens/memories_screen.dart';
import 'ui/screens/mcp_servers_screen.dart';
import 'ui/screens/setup_screen.dart';

void main() async {
  WidgetsFlutterBinding.ensureInitialized();
  final notificationService = NotificationService();
  final foregroundGuard = ForegroundGuard();

  runApp(
    ProviderScope(
      overrides: [
        notificationServiceProvider.overrideWithValue(notificationService),
        foregroundGuardProvider.overrideWithValue(foregroundGuard),
      ],
      child: const LunaApp(),
    ),
  );
}

class LunaApp extends ConsumerStatefulWidget {
  const LunaApp({super.key});

  @override
  ConsumerState<LunaApp> createState() => _LunaAppState();
}

class _LunaAppState extends ConsumerState<LunaApp> with WidgetsBindingObserver {
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    // Note: init() is called automatically in AppController.build() via Future.microtask
    // No need to call it explicitly here
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    super.didChangeAppLifecycleState(state);
    
    if (state == AppLifecycleState.paused ||
        state == AppLifecycleState.detached) {
      // App going to background
      ref.read(appControllerProvider.notifier).setBackgrounded(true);
    } else if (state == AppLifecycleState.resumed) {
      // App resuming from background
      ref.read(appControllerProvider.notifier).setBackgrounded(false);
      // Check connection and reconnect if needed
      unawaited(
        ref.read(appControllerProvider.notifier).checkAndReconnect(),
      );
    }
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final themeConfig = ref.watch(themeConfigProvider);

    return MaterialApp(
      title: 'Luna Mobile',
      themeMode: themeConfig.themeMode,
      theme: AppTheme.lightTheme,
      darkTheme: AppTheme.darkTheme,
      home: const _HomeRouter(),
    );
  }
}

class _HomeRouter extends ConsumerWidget {
  const _HomeRouter();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final state = ref.watch(appControllerProvider);

    final child = switch (state.pane) {
      ActivePane.setup => const SetupScreen(),
      ActivePane.connecting => const ConnectingScreen(),
      ActivePane.conversations => const ConversationsScreen(),
      ActivePane.chat => const ChatScreen(),
      ActivePane.memories => const MemoriesScreen(),
      ActivePane.mcp => const McpServersScreen(),
      ActivePane.settings => const SetupScreen(),
    };

    return Scaffold(
      body: SafeArea(child: child),
    );
  }
}
