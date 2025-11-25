import 'dart:async';

import 'package:audioplayers/audioplayers.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:wakelock_plus/wakelock_plus.dart';

import '../../application/app_controller.dart';
import '../../application/app_state.dart';
import '../../core/config/server_config.dart';
import '../../core/config/tts_preferences.dart';
import '../../services/speech_service.dart';
import '../../services/tts_service.dart';
import '../../utils/text_processing.dart';
import '../widgets/bottom_nav.dart';
import '../widgets/chat_bubble.dart';
import '../widgets/typing_bubble.dart';
import '../widgets/voice_mode_overlay.dart';

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
  late final ProviderSubscription<bool> _dialogModeSubscription;
  Set<String> _completedToolIds = {}; // Track completed tools to avoid replaying
  String? _lastTtsMessageId; // Track last message that was read via TTS
  SpeechService? _speechService;
  String? _currentTranscribedText;

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

    // Listen for dialog mode changes
    _dialogModeSubscription = ref.listenManual<bool>(
      appControllerProvider.select((state) => state.isDialogModeActive),
      (previous, next) {
        if (next && (previous == null || !previous)) {
          _startDialogMode();
        } else if (!next && (previous != null && previous)) {
          _stopDialogMode();
        }
      },
    );
  }

  @override
  void dispose() {
    _streamingSubscription.close();
    _toolCompletionSubscription.close();
    _ttsSubscription.close();
    _dialogModeSubscription.close();
    _typingPlayer.dispose();
    _donePlayer.dispose();
    _sentPlayer.dispose();
    _toolPlayer.dispose();
    _speechService?.dispose();
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
    final state = ref.read(appControllerProvider);
    final ttsPrefs = ref.read(ttsPreferencesProvider);
    
    // In dialog mode, always use TTS. Otherwise, check if TTS is enabled.
    if (!state.isDialogModeActive && !ttsPrefs.enabled) return;

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
    
    final appState = ref.read(appControllerProvider);
    if (appState.isDialogModeActive) {
      // In dialog mode, set state to speaking and resume listening after TTS
      final controller = ref.read(appControllerProvider.notifier);
      controller.setDialogModeState(DialogModeState.speaking);
      await ttsService.speak(cleanText, onComplete: () {
        // TTS finished, resume listening if still in dialog mode
        final currentState = ref.read(appControllerProvider);
        if (currentState.isDialogModeActive) {
          _resumeListening();
        }
      });
    } else {
      await ttsService.speak(cleanText);
    }
  }

  Future<void> _startDialogMode() async {
    debugPrint('ChatScreen: _startDialogMode called');
    final speechService = ref.read(speechServiceProvider);
    _speechService = speechService;
    final ttsPrefs = ref.read(ttsPreferencesProvider);
    final controller = ref.read(appControllerProvider.notifier);
    
    // Enable wakelock to keep screen on
    await WakelockPlus.enable();
    debugPrint('ChatScreen: Wakelock enabled');
    
    // Initialize speech service
    final available = await speechService.isAvailable();
    debugPrint('ChatScreen: Speech service available=$available');
    if (!available) {
      // Speech recognition not available, exit dialog mode
      debugPrint('ChatScreen: Speech not available, exiting dialog mode');
      controller.stopDialogMode();
      return;
    }
    
    // Set up callbacks BEFORE starting to listen
    debugPrint('ChatScreen: Setting up speech callbacks');
    
    speechService.onResult = (text) {
      debugPrint('ChatScreen: onResult callback - text="$text"');
      setState(() {
        _currentTranscribedText = text;
      });
      
      // If user starts speaking during TTS, stop TTS and resume listening
      final currentState = ref.read(appControllerProvider);
      if (currentState.dialogModeState == DialogModeState.speaking && text.trim().isNotEmpty) {
        debugPrint('ChatScreen: User speaking during TTS, stopping TTS');
        final ttsService = ref.read(ttsServiceProvider);
        ttsService.stop();
        controller.setDialogModeState(DialogModeState.listening);
      }
    };
    
    speechService.onPauseDetected = () {
      debugPrint('ChatScreen: onPauseDetected callback triggered!');
      // User paused, send message
      // Get the current text from speech service (most up-to-date)
      final textToSend = _speechService?.currentText ?? _currentTranscribedText ?? '';
      debugPrint('ChatScreen: Text to send: "$textToSend"');
      if (textToSend.trim().isNotEmpty) {
        debugPrint('ChatScreen: Calling _sendVoiceMessage');
        _sendVoiceMessage(textToSend.trim());
        _currentTranscribedText = null;
      } else {
        debugPrint('ChatScreen: No text to send');
      }
    };
    
    speechService.onError = (error) {
      debugPrint('ChatScreen: Speech error: $error');
      // Try to restart listening on error
      if (ref.read(appControllerProvider).isDialogModeActive) {
        debugPrint('ChatScreen: Attempting to restart listening after error');
        Future.delayed(const Duration(milliseconds: 500), () {
          if (ref.read(appControllerProvider).isDialogModeActive) {
            speechService.startListening(ttsPrefs.language);
          }
        });
      }
    };
    
    speechService.onUnexpectedStop = () {
      debugPrint('ChatScreen: STT stopped unexpectedly');
      // Restart listening if still in dialog mode and in listening state
      final currentState = ref.read(appControllerProvider);
      if (currentState.isDialogModeActive && 
          currentState.dialogModeState == DialogModeState.listening) {
        debugPrint('ChatScreen: Restarting listening after unexpected stop');
        Future.delayed(const Duration(milliseconds: 300), () {
          if (ref.read(appControllerProvider).isDialogModeActive) {
            speechService.startListening(ttsPrefs.language);
          }
        });
      }
    };
    
    // Start listening
    debugPrint('ChatScreen: Starting to listen with language=${ttsPrefs.language}');
    controller.setDialogModeState(DialogModeState.listening);
    final started = await speechService.startListening(ttsPrefs.language);
    debugPrint('ChatScreen: startListening returned $started');
  }

  Future<void> _stopDialogMode() async {
    debugPrint('ChatScreen: _stopDialogMode called');
    
    // Disable wakelock
    await WakelockPlus.disable();
    
    // Stop speech recognition and clear callbacks
    await _speechService?.cancel();
    _speechService?.clearCallbacks();
    _speechService = null;
    _currentTranscribedText = null;
    
    debugPrint('ChatScreen: Dialog mode stopped');
  }

  Future<void> _sendVoiceMessage(String text) async {
    debugPrint('ChatScreen: _sendVoiceMessage called with text="$text"');
    final controller = ref.read(appControllerProvider.notifier);
    
    // Switch to processing state
    debugPrint('ChatScreen: Setting state to processing');
    controller.setDialogModeState(DialogModeState.processing);
    
    // Stop listening temporarily
    debugPrint('ChatScreen: Stopping listening');
    await _speechService?.stopListening();
    
    // Send message
    debugPrint('ChatScreen: Sending prompt');
    controller.sendPrompt(text);
    
    // State will switch to speaking when TTS starts (handled in _triggerTtsForLastMessage)
    debugPrint('ChatScreen: _sendVoiceMessage completed');
  }

  Future<void> _resumeListening() async {
    final appState = ref.read(appControllerProvider);
    if (!appState.isDialogModeActive) return;
    
    final controller = ref.read(appControllerProvider.notifier);
    final ttsPrefs = ref.read(ttsPreferencesProvider);
    
    // Switch back to listening state
    controller.setDialogModeState(DialogModeState.listening);
    
    // Resume listening
    await _speechService?.startListening(ttsPrefs.language);
  }

  Future<void> _stopTtsAndResumeListing() async {
    debugPrint('ChatScreen: _stopTtsAndResumeListing called');
    final appState = ref.read(appControllerProvider);
    if (!appState.isDialogModeActive) return;
    
    // Stop TTS
    final ttsService = ref.read(ttsServiceProvider);
    await ttsService.stop();
    
    // Resume listening
    await _resumeListening();
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
              // Voice mode overlay
              if (state.isDialogModeActive)
                VoiceModeOverlay(
                  state: state.dialogModeState,
                  onClose: () {
                    controller.stopDialogMode();
                  },
                  onStopTts: () {
                    _stopTtsAndResumeListing();
                  },
                ),
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
          onVoiceMode: () {
            controller.startDialogMode();
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
  const _Composer({
    required this.controller,
    required this.onSend,
    required this.onVoiceMode,
  });

  final TextEditingController controller;
  final VoidCallback onSend;
  final VoidCallback onVoiceMode;

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
          if (!isStreaming && !state.isDialogModeActive)
            IconButton(
              onPressed: onVoiceMode,
              icon: const Icon(Icons.mic),
              tooltip: 'Voice mode',
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
