import 'dart:async';

import 'package:audioplayers/audioplayers.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../application/app_controller.dart';
import '../../application/app_state.dart';
import '../../core/config/server_config.dart';
import '../../core/config/tts_preferences.dart';
import '../../services/tts_service.dart';
import '../../utils/text_processing.dart';
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
  late final AudioPlayer _sentPlayer;
  late final AudioPlayer _toolPlayer;
  bool _typingActive = false;
  late final ProviderSubscription<bool> _streamingSubscription;
  late final ProviderSubscription<List<ChatMessage>> _toolCompletionSubscription;
  late final ProviderSubscription<List<ChatMessage>> _ttsSubscription;
  Set<String> _completedToolIds = {}; // Track completed tools to avoid replaying
  String? _lastTtsMessageId; // Track last message that was read via TTS

  @override
  void initState() {
    super.initState();
    _typingPlayer = AudioPlayer(playerId: 'typing_indicator')
      ..setReleaseMode(ReleaseMode.stop);
    _donePlayer = AudioPlayer(playerId: 'typing_complete')
      ..setReleaseMode(ReleaseMode.stop);
    _sentPlayer = AudioPlayer(playerId: 'sent_message')
      ..setReleaseMode(ReleaseMode.stop);
    // Preload sent sound for instant playback
    unawaited(_sentPlayer.setSource(AssetSource('audio/sent.mp3')));
    
    _toolPlayer = AudioPlayer(playerId: 'tool_complete')
      ..setReleaseMode(ReleaseMode.stop);
    // Preload tool sound for instant playback
    unawaited(_toolPlayer.setSource(AssetSource('audio/tool.mp3')));

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
    
    // Listen for tool completions
    _toolCompletionSubscription = ref.listenManual<List<ChatMessage>>(
      appControllerProvider.select((state) => state.chatMessages),
      (previous, next) {
        // Check for newly completed tools
        for (final message in next) {
          if (message.toolChip != null && 
              message.toolChip!.status == 'done' &&
              !_completedToolIds.contains(message.toolChip!.id)) {
            _completedToolIds.add(message.toolChip!.id);
            _toolPlayer.stop();
            unawaited(_toolPlayer.play(AssetSource('audio/tool.mp3')));
          }
        }
      },
    );

    // Listen for new assistant messages to trigger TTS
    _ttsSubscription = ref.listenManual<List<ChatMessage>>(
      appControllerProvider.select((state) => state.chatMessages),
      (previous, next) {
        _handleTtsForNewMessage(previous, next);
      },
    );
  }

  @override
  void dispose() {
    _streamingSubscription.close();
    _toolCompletionSubscription.close();
    _ttsSubscription.close();
    _typingPlayer.dispose();
    _donePlayer.dispose();
    _sentPlayer.dispose();
    _toolPlayer.dispose();
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
      _donePlayer.play(AssetSource('audio/done.mp3')).then((_) {
        // After done.mp3 finishes, trigger TTS if enabled
        // Wait a small delay to ensure message is finalized
        Future.delayed(const Duration(milliseconds: 300), () {
          _triggerTtsForLastMessage();
        });
      });
    }
  }

  void _handleTtsForNewMessage(
    List<ChatMessage>? previous,
    List<ChatMessage> next,
  ) {
    // This listener is mainly for tracking message IDs
    // Actual TTS is triggered from _stopTypingFeedback after done.mp3
    final state = ref.read(appControllerProvider);
    if (state.streaming) return;

    // Find the last assistant message
    final lastAssistant = next.lastWhere(
      (m) => m.role == 'assistant' && !m.isStreaming,
      orElse: () => ChatMessage(
        id: '',
        role: 'assistant',
        content: '',
        timestamp: DateTime.now(),
      ),
    );

    // Update tracking for the last message ID
    if (lastAssistant.id.isNotEmpty) {
      // Don't trigger TTS here - it's handled by _stopTypingFeedback
      // This is just for tracking
    }
  }

  Future<void> _triggerTtsForLastMessage() async {
    final ttsPrefs = ref.read(ttsPreferencesProvider);
    if (!ttsPrefs.enabled) return;

    final state = ref.read(appControllerProvider);
    if (state.chatMessages.isEmpty) return;

    // Get the last assistant message
    final lastAssistant = state.chatMessages.lastWhere(
      (m) => m.role == 'assistant' && !m.isStreaming,
      orElse: () => ChatMessage(
        id: '',
        role: 'assistant',
        content: '',
        timestamp: DateTime.now(),
      ),
    );

    if (lastAssistant.content.isEmpty) return;
    
    // Skip if we already read this message
    if (lastAssistant.id == _lastTtsMessageId) return;
    _lastTtsMessageId = lastAssistant.id;

    // Strip emojis and markdown
    final cleanText = stripEmojisAndMarkdown(lastAssistant.content);

    if (cleanText.trim().isEmpty) return;

    // Get TTS service and set language
    final ttsService = ref.read(ttsServiceProvider);
    await ttsService.setLanguage(ttsPrefs.language);
    await ttsService.speak(cleanText);
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

    return PopScope(
      canPop: false,
      onPopInvoked: (didPop) {
        if (!didPop) {
          controller.openConversations();
        }
      },
      child: Column(
      children: [
        _TopBar(
          title: heading,
          connection: state.connection,
          profile: config.profile,
          streaming: state.streaming,
          onSettings: controller.openSetup,
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
            final text = _controller.text;
            if (text.trim().isNotEmpty) {
              // Play sound immediately (preloaded + LOW_LATENCY mode = instant)
              _sentPlayer.stop();
              unawaited(_sentPlayer.play(AssetSource('audio/sent.mp3')));
              controller.sendPrompt(text);
              _controller.clear();
            }
          },
        ),
        LunaBottomBar(
          onConversations: controller.openConversations,
          onStartNew: controller.startNewConversation,
          onSettings: controller.openSettings,
        ),
      ],
    ),
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
                  icon: Icon(
                    Icons.arrow_drop_down,
                    color: Theme.of(context).colorScheme.onSurface,
                  ),
                  elevation: 16,
                  style: TextStyle(color: Theme.of(context).colorScheme.onSurface),
                  dropdownColor: Theme.of(context).colorScheme.surfaceContainer,
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

class _Composer extends ConsumerWidget {
  const _Composer({required this.controller, required this.onSend});

  final TextEditingController controller;
  final VoidCallback onSend;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final state = ref.watch(appControllerProvider);
    final controller = ref.read(appControllerProvider.notifier);
    final isStreaming = state.streaming;

    return Container(
      padding: const EdgeInsets.fromLTRB(16, 8, 16, 16),
      child: Row(
        children: [
          Expanded(
            child: TextField(
              controller: this.controller,
              enabled: !isStreaming,
              minLines: 1,
              maxLines: 5,
              decoration: const InputDecoration(
                hintText: '✏ Message…',
                border: OutlineInputBorder(),
              ),
            ),
          ),
          const SizedBox(width: 8),
          if (isStreaming)
            FilledButton.icon(
              onPressed: () {
                controller.stopStreaming(
                  conversationId: state.activeConversation?.id,
                );
              },
              icon: const Icon(Icons.stop),
              label: const Text('Stop'),
              style: FilledButton.styleFrom(
                backgroundColor: Theme.of(context).colorScheme.error,
              ),
            )
          else
            FilledButton(
              onPressed: onSend,
              child: const Text('⏎ Send'),
            ),
        ],
      ),
    );
  }
}
