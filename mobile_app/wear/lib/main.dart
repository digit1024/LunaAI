import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:wear/wear.dart';

import 'dart:async';
import 'package:luna_mobile/application/app_controller.dart';
import 'package:luna_mobile/application/app_state.dart';
import 'package:luna_mobile/core/config/theme_config.dart';
import 'package:luna_mobile/core/theme/app_theme.dart';
import 'package:luna_mobile/services/notification_service.dart';
import 'package:luna_mobile/services/foreground_guard.dart';
import 'package:luna_mobile/services/tts_service.dart';
import 'wear_chat_screen.dart';
import 'wear_setup_screen.dart';
import 'wear_connecting_screen.dart';
import 'wear_conversations_screen.dart';

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
      child: const LunaWearApp(),
    ),
  );
}

class LunaWearApp extends ConsumerStatefulWidget {
  const LunaWearApp({super.key});

  @override
  ConsumerState<LunaWearApp> createState() => _LunaWearAppState();
}

class _LunaWearAppState extends ConsumerState<LunaWearApp> with WidgetsBindingObserver {
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    super.didChangeAppLifecycleState(state);
    
    if (state == AppLifecycleState.paused ||
        state == AppLifecycleState.detached) {
      ref.read(appControllerProvider.notifier).setBackgrounded(true);
    } else if (state == AppLifecycleState.resumed) {
      ref.read(appControllerProvider.notifier).setBackgrounded(false);
      unawaited(
        ref.read(appControllerProvider.notifier).checkAndReconnect(),
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    final themeConfig = ref.watch(themeConfigProvider);

    return WatchShape(
      builder: (BuildContext context, WearShape shape, Widget? child) {
        return AmbientMode(
          builder: (context, mode, child) {
            // Handle ambient mode - pause TTS when screen dims
            if (mode == WearMode.ambient) {
              WidgetsBinding.instance.addPostFrameCallback((_) {
                final ttsService = ref.read(ttsServiceProvider);
                ttsService.stop();
              });
            }

            return MaterialApp(
              title: 'Luna Wear',
              themeMode: themeConfig.themeMode,
              theme: AppTheme.lightTheme,
              darkTheme: AppTheme.darkTheme,
              home: mode == WearMode.active
                  ? const _WearHomeRouter()
                  : const _AmbientScreen(),
            );
          },
        );
      },
    );
  }
}

class _WearHomeRouter extends ConsumerWidget {
  const _WearHomeRouter();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final state = ref.watch(appControllerProvider);

    final child = switch (state.pane) {
      ActivePane.setup => const WearSetupScreen(),
      ActivePane.connecting => const WearConnectingScreen(),
      ActivePane.conversations => const WearConversationsScreen(),
      ActivePane.chat => const WearChatScreen(),
      ActivePane.memories => const WearConversationsScreen(),
      ActivePane.settings => const WearSetupScreen(),
    };

    return Scaffold(
      body: SafeArea(child: child),
    );
  }
}

class _AmbientScreen extends StatelessWidget {
  const _AmbientScreen();

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: Colors.black,
      body: Center(
          child: Text(
            'Luna',
            style: TextStyle(
              color: Colors.white.withValues(alpha: 0.6),
              fontSize: 24,
              fontWeight: FontWeight.w300,
            ),
          ),
      ),
    );
  }
}

