import 'package:audioplayers/audioplayers.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../application/app_controller.dart';
import '../../application/app_state.dart';
import '../../core/config/server_config.dart';
import '../widgets/bottom_nav.dart';
import '../widgets/chat_bubble.dart';
import '../widgets/typing_bubble.dart';

class ChatScreen extends ConsumerStatefulWidget {
  const ChatScreen({super.key});

  @override
  ConsumerState<ChatScreen> createState() => _ChatScreenState();
}

class _ChatScreenState extends ConsumerState<ChatScreen> {
  final _controller = TextEditingController();
  final _scrollController = ScrollController();
  late final AudioPlayer _typingPlayer;
  late final AudioPlayer _donePlayer;
  bool _typingActive = false;
  late final ProviderSubscription<bool> _streamingSubscription;

  @override
  void initState() {
    super.initState();
    _typingPlayer = AudioPlayer(playerId: 'typing_indicator')
      ..setReleaseMode(ReleaseMode.stop);
    _donePlayer = AudioPlayer(playerId: 'typing_complete')
      ..setReleaseMode(ReleaseMode.stop);

    final streamingProvider =
        appControllerProvider.select((state) => state.streaming);
    if (ref.read(streamingProvider)) _startTypingFeedback();

    _streamingSubscription = ref.listenManual<bool>(
      streamingProvider,
      (previous, next) {
        if (next) {
          if (previous != true) _startTypingFeedback();
        } else if (previous == true) {
          _stopTypingFeedback(playCompletion: true);
        }
      },
    );
  }

  @override
  void dispose() {
    _streamingSubscription.close();
    _typingPlayer.dispose();
    _donePlayer.dispose();
    _controller.dispose();
    _scrollController.dispose();
    super.dispose();
  }

  void _startTypingFeedback() {
    if (_typingActive) return;
    _typingActive = true;
    _typingPlayer.stop();
    _typingPlayer.play(AssetSource('audio/typing.mp3'));
  }

  void _stopTypingFeedback({bool playCompletion = false}) {
    if (_typingActive) {
      _typingActive = false;
      _typingPlayer.stop();
    }
    if (playCompletion) {
      _donePlayer.stop();
      _donePlayer.play(AssetSource('audio/done.mp3'));
    }
  }

  @override
  Widget build(BuildContext context) {
    final state = ref.watch(appControllerProvider);
    final controller = ref.read(appControllerProvider.notifier);
    final config = ref.watch(serverConfigProvider);
    final heading = state.activeConversation?.title ?? 'AI Chat';

    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (_scrollController.hasClients) {
        _scrollController.animateTo(
          _scrollController.position.maxScrollExtent,
          duration: const Duration(milliseconds: 200),
          curve: Curves.easeOut,
        );
      }
    });

    return Column(
      children: [
        _TopBar(
          title: heading,
          connection: state.connection,
          profile: config.profile,
          streaming: state.streaming,
          onSettings: () => _openSettings(context, config),
        ),
        Expanded(
          child: Stack(
            children: [
              Positioned.fill(
                child: ListView.builder(
                  controller: _scrollController,
                  padding: const EdgeInsets.symmetric(
                      horizontal: 16, vertical: 12),
                  itemCount: state.chatMessages.length +
                      (state.streaming ? 1 : 0),
                  itemBuilder: (context, index) {
                    if (index < state.chatMessages.length) {
                      final message = state.chatMessages[index];
                      final isUser = message.role == 'user';
                      return Padding(
                        padding: const EdgeInsets.symmetric(vertical: 6),
                        child: ChatBubble(
                          message: message,
                          isUser: isUser,
                        ),
                      );
                    }
                    return const Padding(
                      padding: EdgeInsets.symmetric(vertical: 6),
                      child: TypingBubble(),
                    );
                  },
                ),
              ),
              if (state.chatMessages.isEmpty && !state.streaming)
                const IgnorePointer(child: _EmptyChat()),
            ],
          ),
        ),
        _Composer(
          controller: _controller,
          onSend: () {
            controller.sendPrompt(_controller.text);
            _controller.clear();
          },
        ),
        LunaBottomBar(
          onConversations: controller.openConversations,
          onStartNew: controller.startNewConversation,
          onSettings: () => _openSettings(context, config),
        ),
      ],
    );
  }

  void _openSettings(BuildContext context, ServerConfig config) {
    showModalBottomSheet(
      context: context,
      builder: (_) => _SettingsSheet(config: config),
    );
  }
}

class _TopBar extends ConsumerWidget {
  const _TopBar({
    required this.title,
    required this.connection,
    required this.profile,
    required this.streaming,
    required this.onSettings,
  });

  final String title;
  final ConnectionStatus connection;
  final String profile;
  final bool streaming;
  final VoidCallback onSettings;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final state = ref.watch(appControllerProvider);
    final controller = ref.read(appControllerProvider.notifier);
    final connectionLabel = switch (connection) {
      ConnectionStatus.connecting => '🔄 Connection: Connecting',
      ConnectionStatus.online => '🟢 Connection: Online',
      ConnectionStatus.error => '🔴 Connection: Error',
    };

    return Container(
      padding: const EdgeInsets.fromLTRB(16, 24, 16, 12),
      decoration: BoxDecoration(
        color: Theme.of(context).colorScheme.surfaceVariant,
        border: Border(
          bottom: BorderSide(color: Theme.of(context).dividerColor),
        ),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(
                  title,
                  style: const TextStyle(
                    fontSize: 20,
                    fontWeight: FontWeight.bold,
                  ),
                ),
              ),
              if (state.availableProfiles.isNotEmpty && state.availableProfiles.contains(profile))
                DropdownButton<String>(
                  value: profile,
                  icon: const Icon(Icons.arrow_drop_down),
                  elevation: 16,
                  style: const TextStyle(color: Colors.black),
                  underline: Container(
                    height: 2,
                    color: Theme.of(context).colorScheme.primary,
                  ),
                  onChanged: (String? newValue) {
                    if (newValue != null && newValue != profile) {
                      controller.changeProfile(newValue);
                    }
                  },
                  items: state.availableProfiles
                      .toSet() // Remove duplicates
                      .map<DropdownMenuItem<String>>((String value) {
                    return DropdownMenuItem<String>(
                      value: value,
                      child: Text('Profile: $value'),
                    );
                  }).toList(),
                )
              else
                TextButton.icon(
                  onPressed: onSettings,
                  icon: const Icon(Icons.person),
                  label: Text('Profile: $profile'),
                ),
            ],
          ),
          const SizedBox(height: 4),
          Row(
            children: [
              Text(connectionLabel),
              if (streaming) const Padding(
                padding: EdgeInsets.only(left: 8),
                child: Text('Streaming…'),
              ),
            ],
          ),
        ],
      ),
    );
  }
}

class _EmptyChat extends StatelessWidget {
  const _EmptyChat();

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          const Icon(Icons.auto_fix_high, size: 48),
          const SizedBox(height: 8),
          const Text('Ready to help'),
          const SizedBox(height: 4),
          Text(
            'Start typing below to begin the agentic loop.',
            style: Theme.of(context).textTheme.bodySmall,
          ),
        ],
      ),
    );
  }
}

class _Composer extends StatelessWidget {
  const _Composer({required this.controller, required this.onSend});

  final TextEditingController controller;
  final VoidCallback onSend;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.fromLTRB(16, 8, 16, 16),
      child: Row(
        children: [
          Expanded(
            child: TextField(
              controller: controller,
              minLines: 1,
              maxLines: 5,
              decoration: const InputDecoration(
                hintText: '✏ Message…',
                border: OutlineInputBorder(),
              ),
            ),
          ),
          const SizedBox(width: 8),
          FilledButton(
            onPressed: onSend,
            child: const Text('⏎ Send'),
          ),
        ],
      ),
    );
  }
}

class _SettingsSheet extends ConsumerWidget {
  const _SettingsSheet({required this.config});

  final ServerConfig config;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final notifier = ref.read(serverConfigProvider.notifier);
    return Padding(
      padding: const EdgeInsets.all(16),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Text(
            'Settings',
            style: TextStyle(fontSize: 18, fontWeight: FontWeight.bold),
          ),
          const SizedBox(height: 12),
          TextFormField(
            initialValue: config.host,
            decoration: const InputDecoration(labelText: 'Server host/IP'),
            onChanged: notifier.updateHost,
          ),
          TextFormField(
            initialValue: config.port.toString(),
            keyboardType: TextInputType.number,
            decoration: const InputDecoration(labelText: 'Port'),
            onChanged: (value) {
              final parsed = int.tryParse(value);
              if (parsed != null) notifier.updatePort(parsed);
            },
          ),
          TextFormField(
            initialValue: config.apiKey,
            decoration: const InputDecoration(labelText: 'API key'),
            onChanged: notifier.updateApiKey,
          ),
          TextFormField(
            initialValue: config.profile,
            decoration: const InputDecoration(labelText: 'Profile'),
            onChanged: (value) {
              notifier.updateProfile(value);
              ref.read(appControllerProvider.notifier).changeProfile(value);
            },
          ),
          const SizedBox(height: 12),
          FilledButton(
            onPressed: () {
              Navigator.of(context).pop();
            },
            child: const Text('Save'),
          ),
        ],
      ),
    );
  }
}

