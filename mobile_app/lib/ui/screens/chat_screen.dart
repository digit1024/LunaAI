import 'dart:async';

import 'dart:io';

import 'package:audioplayers/audioplayers.dart';
import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:wakelock_plus/wakelock_plus.dart';

import '../../application/app_controller.dart';
import '../../application/app_state.dart';
import '../../core/config/server_config.dart';
import '../../data/http/file_client.dart';
import '../../core/config/tts_preferences.dart';
import '../../core/config/tts_provider_type.dart';
import '../../core/config/stt_preferences.dart';
import '../../ui/screens/qween_tts_settings_screen.dart';
import '../../services/speech_service.dart';
import '../../services/tts_provider_resolver.dart';
import '../../services/tts_service.dart';
import '../../utils/text_processing.dart';
import '../../utils/platform_utils.dart';
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
  final _focusNode = FocusNode();
  late final AudioPlayer _typingPlayer;
  late final AudioPlayer _donePlayer;
  late final AudioPlayer _sentPlayer;
  late final AudioPlayer _toolPlayer;
  late final AudioPlayer _bublePlayer;
  bool _typingActive = false;
  late final ProviderSubscription<bool> _streamingSubscription;
  late final ProviderSubscription<List<ChatMessage>> _toolCompletionSubscription;
  late final ProviderSubscription<List<ChatMessage>> _ttsSubscription;
  late final ProviderSubscription<bool> _dialogModeSubscription;
  Set<String> _completedToolIds = {}; // Track completed tools to avoid replaying
  Set<String> _processedMessageIds = {}; // Track messages that have been processed for sound/TTS
  SpeechService? _speechService;
  String? _currentTranscribedText;
  Timer? _sttErrorRetryTimer; // Debounce STT error retries to avoid double start (error_busy)

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
    
    _bublePlayer = AudioPlayer(playerId: 'message_buble')
      ..setReleaseMode(ReleaseMode.stop);
    // Preload buble sound for instant playback
    unawaited(_bublePlayer.setSource(AssetSource('audio/buble.mp3')));

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
          // Turn complete: resume STT in dialog mode ONLY if no TTS will play (empty last message)
          // If TTS will play, resume listening will happen in TTS onComplete callback
          final state = ref.read(appControllerProvider);
          if (state.isDialogModeActive) {
            // Check if last assistant message has content (TTS will play)
            final lastAssistant = state.chatMessages.isEmpty
                ? null
                : state.chatMessages.lastWhere(
                    (m) => m.bubbleType == BubbleType.assistant && !m.isStreaming,
                    orElse: () => ChatMessage(
                      id: '',
                      role: 'assistant',
                      content: '',
                      timestamp: DateTime.now(),
                      bubbleType: BubbleType.assistant,
                    ),
                  );
            final cleanText = lastAssistant != null
                ? stripEmojisAndMarkdown(lastAssistant.content)
                : '';
            
            // Only resume if last message is empty (no TTS will play)
            if (lastAssistant == null || 
                lastAssistant.content.isEmpty || 
                cleanText.trim().isEmpty) {
              debugPrint('ChatScreen: Streaming ended, no TTS (empty message), resuming listening');
              // We woudl never hit this code. empty messgae is unacceptable
            } else {
              debugPrint('ChatScreen: Streaming ended, TTS will play, waiting for TTS completion');
            }
          }
        }
      },
    );
    
    // Listen for tool completions
    _toolCompletionSubscription = ref.listenManual<List<ChatMessage>>(
      appControllerProvider.select((state) => state.chatMessages),
      (previous, next) {
        // Check for newly completed tools (toolResult bubbles)
        for (final message in next) {
          if (message.bubbleType == BubbleType.toolResult && 
              message.toolStatus == 'done' &&
              message.toolCallId != null &&
              !_completedToolIds.contains(message.toolCallId!)) {
            _completedToolIds.add(message.toolCallId!);
            _toolPlayer.stop();
            unawaited(_toolPlayer.play(AssetSource('audio/tool.mp3')));
          }
        }
      },
    );

    // Listen for new assistant messages to trigger TTS or sound
    _ttsSubscription = ref.listenManual<List<ChatMessage>>(
      appControllerProvider.select((state) => state.chatMessages),
      (previous, next) {
        _handleNewAssistantMessages(previous, next);
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
    
    // Set up callback for retry input restoration
    ref.read(appControllerProvider.notifier).onRetryInputReady = (text) {
      _controller.text = text;
      // Move cursor to end of text
      _controller.selection = TextSelection.fromPosition(
        TextPosition(offset: text.length),
      );
      // Focus the input field
      _focusNode.requestFocus();
    };
  }

  @override
  void dispose() {
    _sttErrorRetryTimer?.cancel();
    _sttErrorRetryTimer = null;
    _streamingSubscription.close();
    _toolCompletionSubscription.close();
    _ttsSubscription.close();
    _dialogModeSubscription.close();
    _typingPlayer.dispose();
    _donePlayer.dispose();
    _sentPlayer.dispose();
    _toolPlayer.dispose();
    _bublePlayer.dispose();
    _speechService?.dispose();
    _controller.dispose();
    _scrollController.dispose();
    _focusNode.dispose();
    // Clear retry input callback
    ref.read(appControllerProvider.notifier).onRetryInputReady = null;
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
      // TTS is triggered only from _handleNewAssistantMessages (completed-messages path)
    }
  }

  void _handleNewAssistantMessages(
    List<ChatMessage>? previous,
    List<ChatMessage> next,
  ) {
    final state = ref.read(appControllerProvider);
    final ttsPrefs = ref.read(ttsPreferencesProvider);
    
    // Get previous message IDs for comparison
    final previousIds = previous?.map((m) => m.id).toSet() ?? <String>{};
    
    // Find new assistant messages that just started streaming (bubble begins)
    final newStreamingMessages = next.where(
      (m) => m.bubbleType == BubbleType.assistant && 
             m.isStreaming && 
             !previousIds.contains(m.id),
    ).toList();
    
    // Play bubble sound when message bubble begins (if TTS is disabled)
    if (newStreamingMessages.isNotEmpty && !ttsPrefs.enabled && !state.isDialogModeActive) {
      // Find all assistant messages
      final allAssistantMessages = next.where(
        (m) => m.bubbleType == BubbleType.assistant,
      ).toList();
      final lastAssistantId = allAssistantMessages.lastOrNull?.id;
      
      // Check if the last assistant message is also the absolute last message in the conversation
      final isLastMessageInConversation = lastAssistantId != null && 
          next.isNotEmpty && 
          next.last.id == lastAssistantId;
      
      for (final message in newStreamingMessages) {
        // Skip if already processed
        if (_processedMessageIds.contains(message.id)) continue;
        
        // Only skip if this is the last assistant message AND it's the absolute last message
        // (meaning no tool calls or anything after it) AND it's the only assistant message
        // In this case, done.mp3 will play instead
        final shouldSkip = message.id == lastAssistantId && 
                          isLastMessageInConversation && 
                          allAssistantMessages.length == 1;
        
        if (shouldSkip) {
          continue;
        }
        
        // Play bubble sound for all other assistant messages
        // This includes messages that have tool calls after them
        _processedMessageIds.add(message.id);
        _bublePlayer.stop();
        unawaited(_bublePlayer.play(AssetSource('audio/buble.mp3')));
      }
    }
    
    // Handle TTS for completed messages (single path: only the last assistant message)
    // Last = last assistant (not streaming) in the full conversation
    final lastAssistant = next.isEmpty
        ? null
        : next.lastWhere(
            (m) => m.bubbleType == BubbleType.assistant && !m.isStreaming,
            orElse: () => ChatMessage(
              id: '',
              role: 'assistant',
              content: '',
              timestamp: DateTime.now(),
              bubbleType: BubbleType.assistant,
            ),
          );
    final lastAssistantId = lastAssistant?.id ?? '';
    if (lastAssistantId.isEmpty) return;

    // Only run TTS when that last message just completed in this update
    final completedMessages = next
        .where(
            (m) => m.bubbleType == BubbleType.assistant && !m.isStreaming)
        .where((m) {
      final wasStreaming =
          previous?.any((p) => p.id == m.id && p.isStreaming) ?? false;
      return wasStreaming || !previousIds.contains(m.id);
    }).toList();
    final lastJustCompleted =
        completedMessages.any((m) => m.id == lastAssistantId);

    if (!lastJustCompleted ||
        (!ttsPrefs.enabled && !state.isDialogModeActive)) {
      return;
    }

    if (_processedMessageIds.contains('${lastAssistantId}_tts')) {
      return;
    }
    _processedMessageIds.add('${lastAssistantId}_tts');

    // Empty: don't start TTS; STT will resume when streaming goes false (handled in streaming listener)
    final cleanText = stripEmojisAndMarkdown(lastAssistant!.content);
    if (lastAssistant.content.isEmpty || cleanText.trim().isEmpty) {
      return;
    }

    _playTtsForMessage(lastAssistant);
  }

  Future<void> _playTtsForMessage(ChatMessage message) async {
    if (message.content.isEmpty) return;
    
    final state = ref.read(appControllerProvider);
    
    // Strip emojis and markdown
    final cleanText = stripEmojisAndMarkdown(message.content);
    if (cleanText.trim().isEmpty) return;

    // Get active TTS provider (Built-in or Qween) and stop any ongoing TTS
    final ttsProvider = ref.read(ttsProviderResolver);
    await ttsProvider.stop();
    
    if (state.isDialogModeActive) {
      // In dialog mode, set state to speaking. Resume listening after TTS completes.
      final controller = ref.read(appControllerProvider.notifier);
      controller.setDialogModeState(DialogModeState.speaking);
      final messageIdForResume = message.id;

      // Resume listening only when (1) TTS completed, (2) not streaming, (3) this message is still the last assistant.
      // Prevents resuming when we stopped an earlier message to play the next (onComplete fires for stopped playback).
      await ttsProvider.speak(cleanText, onComplete: () {
        final currentState = ref.read(appControllerProvider);
        if (!currentState.isDialogModeActive || currentState.streaming) return;
        final lastAssistant = currentState.chatMessages.isEmpty
            ? null
            : currentState.chatMessages.lastWhere(
                (m) => m.bubbleType == BubbleType.assistant && !m.isStreaming,
                orElse: () => ChatMessage(
                  id: '',
                  role: 'assistant',
                  content: '',
                  timestamp: DateTime.now(),
                  bubbleType: BubbleType.assistant,
                ),
              );
        if (lastAssistant?.id != messageIdForResume) {
          debugPrint('ChatScreen: TTS completed for earlier message, not resuming (last=${lastAssistant?.id})');
          return;
        }
        debugPrint('ChatScreen: TTS completed for last message, resuming listening');
        if(!currentState.streaming)        _resumeListening();
      });
    } else {
      await ttsProvider.speak(cleanText);
    }
  }

  Future<void> _startDialogMode() async {
    debugPrint('ChatScreen: _startDialogMode called');
    final speechService = ref.read(speechServiceProvider);
    _speechService = speechService;
    final sttPrefs = ref.read(sttPreferencesProvider);
    final controller = ref.read(appControllerProvider.notifier);
    
    // Enable wakelock to keep screen on (mobile only)
    if (isMobile) {
      await WakelockPlus.enable();
      debugPrint('ChatScreen: Wakelock enabled');
    }
    
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
      // final currentState = ref.read(appControllerProvider);
      // if (currentState.dialogModeState == DialogModeState.speaking && text.trim().isNotEmpty) {
      //   debugPrint('ChatScreen: User speaking during TTS, stopping TTS');
      //   final ttsProvider = ref.read(ttsProviderResolver);
      //   ttsProvider.stop();
      //   controller.setDialogModeState(DialogModeState.listening);
      // }
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
      _sttErrorRetryTimer?.cancel();
      _sttErrorRetryTimer = null;
      if (!ref.read(appControllerProvider).isDialogModeActive) return;
      debugPrint('ChatScreen: Scheduling single restart after error (500ms)');
      _sttErrorRetryTimer = Timer(const Duration(milliseconds: 500), () {
        _sttErrorRetryTimer = null;
        if (!ref.read(appControllerProvider).isDialogModeActive) return;
        final currentSttPrefs = ref.read(sttPreferencesProvider);
        speechService.startListening(
          currentSttPrefs.language,
          pauseDuration: currentSttPrefs.pauseDuration,
        );
      });
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
            final currentSttPrefs = ref.read(sttPreferencesProvider);
            speechService.startListening(
              currentSttPrefs.language,
              pauseDuration: currentSttPrefs.pauseDuration,
            );
          }
        });
      }
    };
    
    // Start listening
    debugPrint('ChatScreen: Starting to listen with language=${sttPrefs.language}');
    controller.setDialogModeState(DialogModeState.listening);
    final started = await speechService.startListening(
      sttPrefs.language,
      pauseDuration: sttPrefs.pauseDuration,
    );
    debugPrint('ChatScreen: startListening returned $started');
  }

  Future<void> _stopDialogMode() async {
    debugPrint('ChatScreen: _stopDialogMode called');
    _sttErrorRetryTimer?.cancel();
    _sttErrorRetryTimer = null;

    // Disable wakelock (mobile only)
    if (isMobile) {
      await WakelockPlus.disable();
    }

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
    
    // State will switch to speaking when TTS starts (handled in _handleNewAssistantMessages)
    debugPrint('ChatScreen: _sendVoiceMessage completed');
  }

  Future<void> _resumeListening() async {
    final appState = ref.read(appControllerProvider);
    if (!appState.isDialogModeActive) return;
    
    final controller = ref.read(appControllerProvider.notifier);
    final sttPrefs = ref.read(sttPreferencesProvider);
    
    // Switch back to listening state
    controller.setDialogModeState(DialogModeState.listening);
    
    // Resume listening
    await _speechService?.startListening(
      sttPrefs.language,
      pauseDuration: sttPrefs.pauseDuration,
    );
  }

  Future<void> _stopTtsAndResumeListing() async {
    debugPrint('ChatScreen: _stopTtsAndResumeListing called');
    final appState = ref.read(appControllerProvider);
    if (!appState.isDialogModeActive) return;
    
    // Stop TTS (works for both Built-in and Qween)
    final ttsProvider = ref.read(ttsProviderResolver);
    await ttsProvider.stop();
    
    // Resume listening
    await _resumeListening();
  }

  @override
  Widget build(BuildContext context) {
    final state = ref.watch(appControllerProvider);
    final controller = ref.read(appControllerProvider.notifier);
    final config = ref.watch(serverConfigProvider);
    final heading = state.activeConversation?.title ?? 'AI Chat';

    ref.listen<String?>(
      appControllerProvider.select((s) => s.infoMessage),
      (_, infoMessage) {
        if (infoMessage != null && infoMessage.isNotEmpty) {
          ScaffoldMessenger.of(context).showSnackBar(
            SnackBar(content: Text(infoMessage)),
          );
          controller.clearInfoMessage();
        }
      },
    );

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
      child: Scaffold(
        drawer: _ChatDrawer(
          profile: state.currentProfile.isNotEmpty ? state.currentProfile : config.profile,
          availableProfiles: state.availableProfiles,
          onProfileChanged: (newProfile) {
            controller.changeProfile(newProfile);
          },
          onStartNew: controller.startNewConversation,
          onHistory: controller.openConversations,
          onSetup: controller.openSetup,
        ),
        body: Stack(
          children: [
            // Base layout: top bar + list + composer
            Column(
              children: [
                _TopBar(
                  title: heading,
                  connection: state.connection,
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
                              final prevMessage = index > 0
                                  ? state.chatMessages[index - 1]
                                  : null;
                              final nextMessage = index < state.chatMessages.length - 1
                                  ? state.chatMessages[index + 1]
                                  : null;
                              return ChatBubble(
                                message: message,
                                prevMessage: prevMessage,
                                nextMessage: nextMessage,
                                onRetry: message.bubbleType == BubbleType.user
                                    ? () => controller.retryMessage(message.id)
                                    : null,
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
                  focusNode: _focusNode,
                  attachedFiles: state.attachedFiles,
                  onSend: () {
                    final text = _controller.text;
                    if (text.trim().isNotEmpty) {
                      _focusNode.unfocus();
                      _sentPlayer.stop();
                      unawaited(_sentPlayer.play(AssetSource('audio/sent.mp3')));
                      controller.sendPrompt(text);
                      _controller.clear();
                    }
                  },
                  onVoiceMode: () {
                    controller.startDialogMode();
                  },
                  onAttachFile: () async {
                    final result = await FilePicker.platform.pickFiles(
                      type: FileType.any,
                      allowMultiple: false,
                    );
                    if (result != null && result.files.single.path != null) {
                      final file = File(result.files.single.path!);
                      await controller.attachFile(file);
                    }
                  },
                  onRemoveFile: (fileId) {
                    controller.removeAttachedFile(fileId);
                  },
                ),
              ],
            ),
            // Full-screen voice mode overlay (covers top bar + list + composer)
            // RepaintBoundary isolates overlay into its own layer to avoid BLASTBufferQueue overflow on Android
            if (state.isDialogModeActive)
              Positioned.fill(
                child: RepaintBoundary(
                  child: VoiceModeOverlay(
                  state: state.dialogModeState,
                  onClose: () {
                    controller.stopDialogMode();
                  },
                  onStopTts: () {
                    _stopTtsAndResumeListing();
                  },
                ),
                ),
              ),
          ],
        ),
      ),
    );
  }
}

class _TopBar extends ConsumerWidget {
  const _TopBar({
    required this.title,
    required this.connection,
    required this.streaming,
    required this.onSettings,
  });

  final String title;
  final ConnectionStatus connection;
  final bool streaming;
  final VoidCallback onSettings;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final connectionIcon = switch (connection) {
      ConnectionStatus.connecting => Icons.sync,
      ConnectionStatus.online => Icons.circle,
      ConnectionStatus.error => Icons.error,
    };
    final connectionColor = switch (connection) {
      ConnectionStatus.connecting => Colors.orange,
      ConnectionStatus.online => Colors.green,
      ConnectionStatus.error => Colors.red,
    };

    return Container(
      padding: const EdgeInsets.fromLTRB(16, 24, 16, 12),
      decoration: BoxDecoration(
        color: Theme.of(context).colorScheme.surfaceVariant,
        border: Border(
          bottom: BorderSide(color: Theme.of(context).dividerColor),
        ),
      ),
      child: Row(
        children: [
          Builder(
            builder: (context) => IconButton(
              icon: const Icon(Icons.menu),
              onPressed: () => Scaffold.of(context).openDrawer(),
            ),
          ),
          Expanded(
            child: Text(
              title,
              style: const TextStyle(
                fontSize: 18,
                fontWeight: FontWeight.bold,
              ),
            ),
          ),
          Icon(
            connectionIcon,
            color: connectionColor,
            size: 16,
          ),
          if (streaming) const Padding(
            padding: EdgeInsets.only(left: 8),
            child: Text('Streaming…', style: TextStyle(fontSize: 12)),
          ),
        ],
      ),
    );
  }
}

class _ChatDrawer extends ConsumerStatefulWidget {
  const _ChatDrawer({
    required this.profile,
    required this.availableProfiles,
    required this.onProfileChanged,
    required this.onStartNew,
    required this.onHistory,
    required this.onSetup,
  });

  final String profile;
  final List<String> availableProfiles;
  final Function(String) onProfileChanged;
  final VoidCallback onStartNew;
  final VoidCallback onHistory;
  final VoidCallback onSetup;

  @override
  ConsumerState<_ChatDrawer> createState() => _ChatDrawerState();
}

class _ChatDrawerState extends ConsumerState<_ChatDrawer> {
  List<dynamic>? _availableLanguages;
  bool _loadingLanguages = false;

  @override
  void initState() {
    super.initState();
    _loadLanguages();
  }

  Future<void> _loadLanguages() async {
    setState(() {
      _loadingLanguages = true;
    });
    try {
      final ttsService = ref.read(ttsServiceProvider);
      final languages = await ttsService.getLanguages();
      if (mounted) {
        setState(() {
          _availableLanguages = languages;
          _loadingLanguages = false;
        });
      }
    } catch (e) {
      if (mounted) {
        setState(() {
          _loadingLanguages = false;
        });
      }
    }
  }

  String _getLanguageDisplayName(String languageCode) {
    final parts = languageCode.split('-');
    final lang = parts[0];
    final country = parts.length > 1 ? parts[1] : null;

    final languageNames = {
      'en': 'English',
      'es': 'Spanish',
      'fr': 'French',
      'de': 'German',
      'it': 'Italian',
      'pt': 'Portuguese',
      'ru': 'Russian',
      'ja': 'Japanese',
      'ko': 'Korean',
      'zh': 'Chinese',
      'ar': 'Arabic',
      'hi': 'Hindi',
      'nl': 'Dutch',
      'pl': 'Polish',
      'tr': 'Turkish',
      'sv': 'Swedish',
      'da': 'Danish',
      'fi': 'Finnish',
      'no': 'Norwegian',
      'cs': 'Czech',
      'hu': 'Hungarian',
      'ro': 'Romanian',
      'el': 'Greek',
      'he': 'Hebrew',
      'th': 'Thai',
      'vi': 'Vietnamese',
    };

    final langName = languageNames[lang] ?? lang.toUpperCase();
    if (country != null) {
      return '$langName ($country)';
    }
    return langName;
  }

  @override
  Widget build(BuildContext context) {
    final sttPrefs = ref.watch(sttPreferencesProvider);
    final sttPrefsNotifier = ref.read(sttPreferencesProvider.notifier);
    final ttsPrefs = ref.watch(ttsPreferencesProvider);
    final ttsPrefsNotifier = ref.read(ttsPreferencesProvider.notifier);

    return Drawer(
      child: ListView(
        padding: EdgeInsets.zero,
        children: [
          DrawerHeader(
            decoration: BoxDecoration(
              color: Theme.of(context).colorScheme.primaryContainer,
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                Text(
                  'Luna AI',
                  style: Theme.of(context).textTheme.headlineSmall?.copyWith(
                    color: Theme.of(context).colorScheme.onPrimaryContainer,
                  ),
                ),
              ],
            ),
          ),
          // New Chat
          ListTile(
            leading: const Icon(Icons.add),
            title: const Text('New Chat'),
            onTap: () {
              widget.onStartNew();
              Navigator.pop(context);
            },
          ),
          const Divider(),
          // Profile Selection
          if (widget.availableProfiles.isNotEmpty)
            ExpansionTile(
              leading: const Icon(Icons.person),
              title: const Text('Profile'),
              subtitle: Text(widget.profile),
              children: widget.availableProfiles.map((profile) {
                return ListTile(
                  title: Text(profile),
                  selected: profile == widget.profile,
                  onTap: () {
                    widget.onProfileChanged(profile);
                    Navigator.pop(context);
                  },
                );
              }).toList(),
            ),
          // TTS Toggle
          SwitchListTile(
            secondary: const Icon(Icons.volume_up),
            title: const Text('Text-to-Speech'),
            value: ttsPrefs.enabled,
            onChanged: (value) {
              ttsPrefsNotifier.setEnabled(value);
            },
          ),
          // TTS Provider: Built-in vs Qween
          ListTile(
            leading: const Icon(Icons.record_voice_over),
            title: const Text('TTS Provider'),
            subtitle: Text(ttsPrefs.providerType.label),
            onTap: () {
              showModalBottomSheet<void>(
                context: context,
                builder: (ctx) => SafeArea(
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    children: TtsProviderType.values
                        .map((t) => ListTile(
                              title: Text(t.label),
                              onTap: () {
                                ttsPrefsNotifier.setProviderType(t);
                                Navigator.pop(ctx);
                              },
                            ))
                        .toList(),
                  ),
                ),
              );
            },
          ),
          // Qween TTS settings - show when Qween selected
          if (ttsPrefs.providerType == TtsProviderType.qween)
            ListTile(
              leading: const Icon(Icons.settings_voice),
              title: const Text('Qween TTS Settings'),
              onTap: () {
                Navigator.pop(context);
                Navigator.of(context).push(
                  MaterialPageRoute<void>(
                    builder: (_) => const QweenTtsSettingsScreen(),
                  ),
                );
              },
            ),
          const Divider(),
          // Language Selection (applies to STT and Built-in TTS only)
          // Hide when Qween TTS - Qween uses English fixed
          if (ttsPrefs.providerType != TtsProviderType.qween)
            ListTile(
              leading: const Icon(Icons.language),
              title: const Text('Voice Language'),
              subtitle: _loadingLanguages
                  ? const Text('Loading...')
                  : (_availableLanguages != null && _availableLanguages!.isNotEmpty)
                      ? Builder(
                          builder: (context) {
                            // Filter to only show favorite languages
                            final favoriteLanguages = sttPrefs.favoriteLanguages;
                            final favoriteLangItems = _availableLanguages!
                                .where((lang) {
                                  final langCode = lang.toString();
                                  return favoriteLanguages.contains(langCode);
                                })
                                .toList();
                            
                            // If current language is not in favorites, add it temporarily
                            final currentLangCode = ttsPrefs.language;
                            if (!favoriteLanguages.contains(currentLangCode)) {
                              favoriteLangItems.insert(0, currentLangCode);
                            }
                            
                            if (favoriteLangItems.isEmpty) {
                              return TextButton.icon(
                                onPressed: _loadLanguages,
                                icon: const Icon(Icons.refresh, size: 18),
                                label: const Text('Load Languages'),
                              );
                            }
                            
                            return DropdownButton<String>(
                              value: ttsPrefs.language,
                              isExpanded: true,
                              underline: Container(),
                              items: favoriteLangItems
                                  .map<DropdownMenuItem<String>>((lang) {
                                final langCode = lang.toString();
                                final displayName = _getLanguageDisplayName(langCode);
                                return DropdownMenuItem<String>(
                                  value: langCode,
                                  child: Text(displayName),
                                );
                              }).toList(),
                              onChanged: (value) {
                                if (value != null) {
                                  // Set both TTS and STT language
                                  ttsPrefsNotifier.setLanguage(value);
                                  sttPrefsNotifier.setLanguage(value);
                                  ref.read(ttsServiceProvider).setLanguage(value);
                                }
                              },
                            );
                          },
                        )
                      : TextButton.icon(
                          onPressed: _loadLanguages,
                          icon: const Icon(Icons.refresh, size: 18),
                          label: const Text('Load Languages'),
                        ),
            ),
          const Divider(),
          // History
          ListTile(
            leading: const Icon(Icons.history),
            title: const Text('History'),
            onTap: () {
              widget.onHistory();
              Navigator.pop(context);
            },
          ),
          // Setup
          ListTile(
            leading: const Icon(Icons.settings),
            title: const Text('Setup'),
            onTap: () {
              widget.onSetup();
              Navigator.pop(context);
            },
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

class _Composer extends ConsumerStatefulWidget {
  const _Composer({
    required this.controller,
    required this.focusNode,
    required this.onSend,
    required this.onVoiceMode,
    required this.attachedFiles,
    required this.onAttachFile,
    required this.onRemoveFile,
  });

  final TextEditingController controller;
  final FocusNode focusNode;
  final VoidCallback onSend;
  final VoidCallback onVoiceMode;
  final List<FileAttachment> attachedFiles;
  final VoidCallback onAttachFile;
  final Function(String) onRemoveFile;

  @override
  ConsumerState<_Composer> createState() => _ComposerState();
}

class _ComposerState extends ConsumerState<_Composer> {
  @override
  void initState() {
    super.initState();
    widget.controller.addListener(_onTextChanged);
  }

  @override
  void dispose() {
    widget.controller.removeListener(_onTextChanged);
    super.dispose();
  }

  void _onTextChanged() {
    setState(() {}); // Rebuild to update send button visibility
  }

  @override
  Widget build(BuildContext context) {
    final state = ref.watch(appControllerProvider);
    final controller = ref.read(appControllerProvider.notifier);
    final isStreaming = state.streaming;
    final colorScheme = Theme.of(context).colorScheme;
    final hasText = widget.controller.text.trim().isNotEmpty;

    return Container(
      decoration: BoxDecoration(
        color: colorScheme.surfaceContainerHighest,
        borderRadius: const BorderRadius.only(
          topLeft: Radius.circular(20),
          topRight: Radius.circular(20),
        ),
      ),
      padding: const EdgeInsets.fromLTRB(16, 12, 16, 16),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          // Display attached files
          if (widget.attachedFiles.isNotEmpty)
            Container(
              margin: const EdgeInsets.only(bottom: 8),
              child: Wrap(
                spacing: 8,
                runSpacing: 8,
                children: widget.attachedFiles.map<Widget>((attachment) {
                  return Chip(
                    label: Text(
                      attachment.fileName,
                      style: const TextStyle(fontSize: 12),
                    ),
                    avatar: const Icon(
                      Icons.attach_file,
                      size: 18,
                    ),
                    deleteIcon: const Icon(Icons.close, size: 18),
                    onDeleted: () => widget.onRemoveFile(attachment.fileId),
                  );
                }).toList(),
              ),
            ),
          // Input row - full width
          TextField(
            controller: widget.controller,
            focusNode: widget.focusNode,
            enabled: !isStreaming,
            minLines: 1,
            maxLines: 5,
            textInputAction: TextInputAction.newline,
            style: Theme.of(context).textTheme.bodyMedium,
            decoration: InputDecoration(
              hintText: 'Send message to Luna AI ...',
              hintStyle: TextStyle(
                color: colorScheme.onSurfaceVariant.withOpacity(0.6),
              ),
              filled: true,
              fillColor: Colors.transparent,
              border: InputBorder.none,
              enabledBorder: InputBorder.none,
              focusedBorder: InputBorder.none,
              disabledBorder: InputBorder.none,
              contentPadding: const EdgeInsets.symmetric(
                horizontal: 16,
                vertical: 12,
              ),
            ),
          ),
          // Buttons row - below input
          Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              // Left side - Attachment and Mic buttons
              Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  // Attach file button (+ icon)
                  if (!isStreaming && !state.isDialogModeActive)
                    IconButton(
                      onPressed: widget.onAttachFile,
                      icon: const Icon(Icons.add),
                      tooltip: 'Attach file',
                    ),
                  // Voice mode button (mobile only)
                  if (isMobile && !isStreaming && !state.isDialogModeActive)
                    IconButton(
                      onPressed: widget.onVoiceMode,
                      icon: const Icon(Icons.mic),
                      tooltip: 'Voice mode',
                    ),
                ],
              ),
              // Right side - Send/Stop button
              if (isStreaming)
                IconButton(
                  onPressed: () {
                    controller.stopStreaming(
                      conversationId: state.activeConversation?.id,
                    );
                  },
                  icon: const Icon(Icons.stop),
                  style: IconButton.styleFrom(
                    backgroundColor: colorScheme.error,
                    foregroundColor: colorScheme.onError,
                  ),
                  tooltip: 'Stop',
                )
              else if (hasText)
                IconButton.filled(
                  onPressed: widget.onSend,
                  icon: const Icon(Icons.send),
                  tooltip: 'Send',
                )
              else
                const SizedBox.shrink(),
            ],
          ),
        ],
      ),
    );
  }
}
