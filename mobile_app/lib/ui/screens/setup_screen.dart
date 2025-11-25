import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../application/app_controller.dart';
import '../../core/config/server_config.dart';

class SetupScreen extends ConsumerWidget {
  const SetupScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final config = ref.watch(serverConfigProvider);
    final controller = ref.read(serverConfigProvider.notifier);
    final appController = ref.read(appControllerProvider.notifier);

    return Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(
            'Welcome to Luna',
            style: Theme.of(context).textTheme.headlineMedium,
          ),
          const SizedBox(height: 16),
          ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 300),
            child: Column(
              children: [
                TextField(
                  controller: TextEditingController(text: config.host),
                  decoration: const InputDecoration(
                    labelText: 'Host',
                    border: OutlineInputBorder(),
                  ),
                  onChanged: controller.updateHost,
                ),
                const SizedBox(height: 16),
                TextField(
                  controller: TextEditingController(text: config.port.toString()),
                  decoration: const InputDecoration(
                    labelText: 'Port',
                    border: OutlineInputBorder(),
                  ),
                  onChanged: (value) {
                    final port = int.tryParse(value);
                    if (port != null) {
                      controller.updatePort(port);
                    }
                  },
                ),
                const SizedBox(height: 16),
                TextField(
                  controller: TextEditingController(text: config.apiKey),
                  decoration: const InputDecoration(
                    labelText: 'API Key',
                    border: OutlineInputBorder(),
                  ),
                  onChanged: controller.updateApiKey,
                ),
              ],
            ),
          ),
          const SizedBox(height: 24),
          FilledButton(
            onPressed: appController.connect,
            child: const Text('Connect'),
          )
        ],
      ),
    );
  }
}
