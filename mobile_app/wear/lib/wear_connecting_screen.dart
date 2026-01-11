import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:luna_mobile/application/app_controller.dart';

class WearConnectingScreen extends ConsumerWidget {
  const WearConnectingScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final state = ref.watch(appControllerProvider);
    final attempt = state.connectionAttempt;

    return Scaffold(
      body: Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const CircularProgressIndicator(),
            const SizedBox(height: 16),
            Text(
              attempt > 0 ? 'Connecting ($attempt/3)...' : 'Connecting...',
              style: const TextStyle(fontSize: 12),
            ),
            if (state.error != null) ...[
              const SizedBox(height: 8),
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 16),
                child: Text(
                  state.error!,
                  style: TextStyle(
                    fontSize: 10,
                    color: Colors.red.shade300,
                  ),
                  textAlign: TextAlign.center,
                ),
              ),
            ],
          ],
        ),
      ),
    );
  }
}

