import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../application/app_controller.dart';
import 'vocabulary_screen.dart';
import 'voice_settings_screen.dart';

class SettingsScreen extends ConsumerWidget {
  const SettingsScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final controller = ref.read(appControllerProvider.notifier);

    return Scaffold(
      appBar: AppBar(
        title: const Text('Settings'),
        leading: IconButton(
          icon: const Icon(Icons.arrow_back),
          onPressed: () {
            final navigator = Navigator.of(context);
            if (navigator.canPop()) {
              navigator.pop();
            } else {
              controller.openConversations();
            }
          },
        ),
      ),
      body: ListView(
        children: [
          ListTile(
            leading: const Icon(Icons.mic_none),
            title: const Text('Voice Mode'),
            subtitle: const Text('Input mode, STT, TTS and providers'),
            trailing: const Icon(Icons.chevron_right),
            onTap: () {
              Navigator.of(context).push(
                MaterialPageRoute<void>(
                  builder: (_) => const VoiceSettingsScreen(),
                ),
              );
            },
          ),
          ListTile(
            leading: const Icon(Icons.text_fields),
            title: const Text('Vocabulary'),
            subtitle: const Text('Hint the STT model toward names and terms'),
            trailing: const Icon(Icons.chevron_right),
            onTap: () {
              Navigator.of(context).push(
                MaterialPageRoute<void>(
                  builder: (_) => const VocabularyScreen(),
                ),
              );
            },
          ),
          const Divider(),
          ListTile(
            leading: const Icon(Icons.dns_outlined),
            title: const Text('Connection & Appearance'),
            subtitle: const Text('Server address, API key, theme'),
            trailing: const Icon(Icons.chevron_right),
            onTap: () {
              final navigator = Navigator.of(context);
              if (navigator.canPop()) navigator.pop();
              controller.openSetup();
            },
          ),
          const Divider(),
          const Padding(
            padding: EdgeInsets.all(16),
            child: Text(
              'Voice recognition and speech synthesis run on this device only. '
              'Luna server settings are not involved.',
              style: TextStyle(fontSize: 12),
            ),
          ),
        ],
      ),
    );
  }
}
