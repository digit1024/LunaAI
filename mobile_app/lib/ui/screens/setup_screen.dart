import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../application/app_controller.dart';
import '../../application/app_state.dart';
import '../../core/config/server_config.dart';

class SetupScreen extends ConsumerStatefulWidget {
  const SetupScreen({super.key});

  @override
  ConsumerState<SetupScreen> createState() => _SetupScreenState();
}

class _SetupScreenState extends ConsumerState<SetupScreen> {
  late final TextEditingController _hostController;
  late final TextEditingController _portController;
  late final TextEditingController _apiKeyController;

  @override
  void initState() {
    super.initState();
    final config = ref.read(serverConfigProvider);
    _hostController = TextEditingController(text: config.host);
    _portController = TextEditingController(text: config.port.toString());
    _apiKeyController = TextEditingController(text: config.apiKey);
  }

  @override
  void dispose() {
    _hostController.dispose();
    _portController.dispose();
    _apiKeyController.dispose();
    super.dispose();
  }

  void _updateControllersFromConfig(ServerConfig config) {
    if (_hostController.text != config.host) {
      _hostController.text = config.host;
    }
    if (_portController.text != config.port.toString()) {
      _portController.text = config.port.toString();
    }
    if (_apiKeyController.text != config.apiKey) {
      _apiKeyController.text = config.apiKey;
    }
  }

  @override
  Widget build(BuildContext context) {
    final appState = ref.watch(appControllerProvider);
    final config = ref.watch(serverConfigProvider);
    final error = appState.error;

    // Update controllers when config changes (e.g., when loaded from prefs)
    _updateControllersFromConfig(config);

    final serverConfigNotifier = ref.read(serverConfigProvider.notifier);
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
          if (error != null && appState.pane == ActivePane.setup) ...[
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 16.0),
              child: Text(
                error,
                style: TextStyle(color: Theme.of(context).colorScheme.error),
                textAlign: TextAlign.center,
              ),
            ),
            const SizedBox(height: 16),
          ],
          ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 300),
            child: Column(
              children: [
                TextField(
                  controller: _hostController,
                  decoration: const InputDecoration(
                    labelText: 'Host',
                    border: OutlineInputBorder(),
                  ),
                  onChanged: serverConfigNotifier.updateHost,
                ),
                const SizedBox(height: 16),
                TextField(
                  controller: _portController,
                  keyboardType: TextInputType.number,
                  decoration: const InputDecoration(
                    labelText: 'Port',
                    border: OutlineInputBorder(),
                  ),
                  onChanged: (value) {
                    final port = int.tryParse(value);
                    if (port != null) {
                      serverConfigNotifier.updatePort(port);
                    }
                  },
                ),
                const SizedBox(height: 16),
                TextField(
                  controller: _apiKeyController,
                  decoration: const InputDecoration(
                    labelText: 'API Key',
                    border: OutlineInputBorder(),
                  ),
                  onChanged: serverConfigNotifier.updateApiKey,
                ),
              ],
            ),
          ),
          const SizedBox(height: 24),
          FilledButton(
            onPressed: appState.connection == ConnectionStatus.connecting
                ? null
                : () => appController.connect(),
            child: Text(
              appState.connection == ConnectionStatus.connecting
                  ? 'Connecting...'
                  : 'Connect',
            ),
          )
        ],
      ),
    );
  }
}
