import 'dart:async';
import 'dart:io';

import 'package:audioplayers/audioplayers.dart';
import 'package:file_picker/file_picker.dart';
import 'package:flutter/foundation.dart' show compute;
import 'package:flutter/material.dart';
import 'package:image/image.dart' as img;
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:wakelock_plus/wakelock_plus.dart';

import '../../application/app_controller.dart';
import '../../application/app_state.dart';
import '../../core/config/server_config.dart';
import '../../core/theme/app_theme.dart';
import '../../data/http/file_client.dart'
    show
        FileAttachment,
        kImageExtensions,
        kImageDownscaleThresholdBytes,
        kMaxFileSizeBytes;
import '../../services/tts_provider_resolver.dart';
import '../../tts/message_speech.dart';
import '../../utils/platform_utils.dart';
import '../../voice/stt_service.dart';
import '../../voice/voice_catalog.dart';
import '../../voice/voice_preferences.dart';
import '../../voice/voice_recorder.dart';
import '../widgets/chat_bubble.dart';
import '../widgets/typing_bubble.dart';
import '../widgets/voice_mode_overlay.dart';
import 'settings_screen.dart';

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
  late final ProviderSubscription<bool> _voiceModeSubscription;
  final Set<String> _completedToolIds = {}; // Track completed tools to avoid replaying
  final Set<String> _processedMessageIds = {}; // Track messages that have been processed for sound/TTS
  Set<String>? _messageIdsAtStreamingStart;
  String? _streamingConversationId;
  /// IDs of messages that arrived as part of a bulk conversation load. These
  /// must not play the slide-in animation (we don't want to animate dozens
  /// of historical bubbles every time the user opens a long chat).
  final Set<String> _historicalMessageIds = {};
  /// Conversation we currently believe the screen is showing. When it changes
  /// we treat the next chatMessages snapshot as a bulk load and gate listeners.
  String? _renderedConversationId;
  /// True for one frame after a bulk load so the post-frame scroller jumps to
  /// the bottom instead of running an animateTo against a moving extent.
  bool _pendingJumpToBottom = false;
  /// Tracks the last chatMessages count we observed so we only auto-scroll
  /// when the list actually grows (a new bubble/streaming delta), not on
  /// every rebuild.
  int _lastObservedMessageCount = 0;
  late final VoiceRecorderService _recorder;
  Timer? _voiceTimer;
  DateTime? _listeningStartedAt;
  int _elapsedSeconds = 0;
  bool _voiceStarting = false;

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
    if (ref.read(streamingProvider)) {
      _startTypingFeedback();
      _beginStreamingTurn(ref.read(appControllerProvider));
    }

    _streamingSubscription = ref.listenManual<bool>(
      streamingProvider,
      (previous, next) {
        if (next) {
          if (previous != true) {
            _startTypingFeedback();
            _beginStreamingTurn(ref.read(appControllerProvider));
          }
        } else if (previous == true) {
          _stopTypingFeedback(playCompletion: true);
          final state = ref.read(appControllerProvider);
          _finishStreamingTurn(state);
          if (state.isVoiceModeActive &&
              state.voiceModeState == VoiceModeState.thinking) {
            // TTS (if configured) starts synchronously when the assistant
            // message completes. If we are still thinking shortly after,
            // no speech is going to play.
            Future<void>.delayed(const Duration(milliseconds: 700), () {
              if (!mounted) return;
              final current = ref.read(appControllerProvider);
              if (current.isVoiceModeActive &&
                  current.voiceModeState == VoiceModeState.thinking) {
                ref
                    .read(appControllerProvider.notifier)
                    .setVoiceModeState(VoiceModeState.idle);
              }
            });
          }
        }
      },
    );
    
    // Listen for tool completions
    _toolCompletionSubscription = ref.listenManual<List<ChatMessage>>(
      appControllerProvider.select((state) => state.chatMessages),
      (previous, next) {
        if (_isBulkLoad(previous, next)) {
          // Bulk load: silently absorb the existing tool IDs so we never
          // replay historical tool sounds.
          for (final m in next) {
            if (m.toolCallId != null) _completedToolIds.add(m.toolCallId!);
          }
          return;
        }
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
        if (_isBulkLoad(previous, next)) {
          // Bulk load: pre-mark every existing message (incl. last assistant)
          // as already processed so we don't auto-TTS / play sounds for
          // history.
          for (final m in next) {
            _processedMessageIds.add(m.id);
            _processedMessageIds.add('${m.id}_tts');
          }
          return;
        }
        _handleNewAssistantMessages(previous, next);
      },
    );

    _recorder = VoiceRecorderService();

    // Listen for voice mode changes
    _voiceModeSubscription = ref.listenManual<bool>(
      appControllerProvider.select((state) => state.isVoiceModeActive),
      (previous, next) {
        if (next && (previous == null || !previous)) {
          _enterVoiceMode();
        } else if (!next && (previous != null && previous)) {
          _exitVoiceMode();
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
    _voiceTimer?.cancel();
    _voiceTimer = null;
    _voiceModeSubscription.close();
    ref.read(appControllerProvider.notifier).stopVoiceMode();
    _streamingSubscription.close();
    _toolCompletionSubscription.close();
    _ttsSubscription.close();
    _typingPlayer.dispose();
    _donePlayer.dispose();
    _sentPlayer.dispose();
    _toolPlayer.dispose();
    _bublePlayer.dispose();
    _recorder.dispose();
    _controller.dispose();
    _scrollController.dispose();
    _focusNode.dispose();
    // Clear retry input callback
    ref.read(appControllerProvider.notifier).onRetryInputReady = null;
    super.dispose();
  }

  /// True when [next] is the result of switching/loading a conversation
  /// rather than an incremental update (streaming delta, tool event, etc.).
  /// We treat both "first-ever frame" (previous == null) and any frame where
  /// the message list shrinks or jumps by more than one as a bulk load.
  bool _isBulkLoad(List<ChatMessage>? previous, List<ChatMessage> next) {
    if (previous == null) return next.isNotEmpty;
    if (previous.isEmpty && next.isNotEmpty) return true;
    if (next.length < previous.length) return true; // truncation/retry/switch
    if (next.length - previous.length > 1) return true;
    if (previous.isNotEmpty && next.isNotEmpty && previous.first.id != next.first.id) {
      // Different conversation head — fresh load.
      return true;
    }
    return false;
  }

  /// Snapshot the IDs from a fresh conversation so the bubble widget skips
  /// the slide-in animation for them.
  void _markHistoricalMessages(List<ChatMessage> messages) {
    _historicalMessageIds
      ..clear()
      ..addAll(messages.map((m) => m.id));
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
      // TTS starts from _finishStreamingTurn at the same turn-complete moment.
    }
  }

  void _beginStreamingTurn(AppState state) {
    _streamingConversationId = state.activeConversation?.id;
    _messageIdsAtStreamingStart =
        state.chatMessages.map((message) => message.id).toSet();
  }

  /// Reads the final assistant message once the whole turn has completed —
  /// the same lifecycle moment that plays done.mp3.
  void _finishStreamingTurn(AppState state) {
    final snapshot = _messageIdsAtStreamingStart;
    final conversationId = _streamingConversationId;
    _messageIdsAtStreamingStart = null;
    _streamingConversationId = null;

    if (snapshot == null) return;
    if (conversationId != state.activeConversation?.id) {
      // A conversation switch/load cleared streaming; this is not a live turn.
      if (state.isVoiceModeActive &&
          state.voiceModeState == VoiceModeState.thinking) {
        ref
            .read(appControllerProvider.notifier)
            .setVoiceModeState(VoiceModeState.idle);
      }
      return;
    }

    final lastAssistant = state.chatMessages.isEmpty
        ? null
        : state.chatMessages.cast<ChatMessage?>().lastWhere(
              (m) =>
                  m != null &&
                  m.bubbleType == BubbleType.assistant &&
                  !m.isStreaming,
              orElse: () => null,
            );

    if (lastAssistant == null || snapshot.contains(lastAssistant.id)) {
      if (state.isVoiceModeActive &&
          state.voiceModeState == VoiceModeState.thinking) {
        ref
            .read(appControllerProvider.notifier)
            .setVoiceModeState(VoiceModeState.idle);
      }
      return;
    }

    final cleanText = prepareMessageForTts(lastAssistant.content);
    final voicePrefs = ref.read(voicePreferencesProvider);
    if (!voicePrefs.ttsEnabled ||
        lastAssistant.content.isEmpty ||
        cleanText.trim().isEmpty) {
      if (state.isVoiceModeActive &&
          state.voiceModeState == VoiceModeState.thinking) {
        ref
            .read(appControllerProvider.notifier)
            .setVoiceModeState(VoiceModeState.idle);
      }
      return;
    }

    final key = '${lastAssistant.id}_tts';
    if (_processedMessageIds.contains(key)) return;
    _processedMessageIds.add(key);
    _playTtsForMessage(lastAssistant);
  }

  void _handleNewAssistantMessages(
    List<ChatMessage>? previous,
    List<ChatMessage> next,
  ) {
    final state = ref.read(appControllerProvider);
    final voicePrefs = ref.read(voicePreferencesProvider);
    
    // Get previous message IDs for comparison
    final previousIds = previous?.map((m) => m.id).toSet() ?? <String>{};
    
    // Find new assistant messages that just started streaming (bubble begins)
    final newStreamingMessages = next.where(
      (m) => m.bubbleType == BubbleType.assistant && 
             m.isStreaming && 
             !previousIds.contains(m.id),
    ).toList();
    
    // Play bubble sound when message bubble begins (if TTS is disabled)
    if (newStreamingMessages.isNotEmpty &&
        !voicePrefs.ttsEnabled &&
        !state.isVoiceModeActive) {
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
    
  }

  Future<void> _playTtsForMessage(ChatMessage message) async {
    final cleanText = prepareMessageForTts(message.content);
    final state = ref.read(appControllerProvider);
    if (cleanText.trim().isEmpty) {
      if (state.isVoiceModeActive) {
        ref
            .read(appControllerProvider.notifier)
            .setVoiceModeState(VoiceModeState.idle);
      }
      return;
    }

    final ttsProvider = ref.read(ttsProviderResolver);
    await ttsProvider.stop();

    if (!state.isVoiceModeActive) {
      await ttsProvider.speak(cleanText);
      return;
    }

    final controller = ref.read(appControllerProvider.notifier);
    controller.setVoiceModeState(VoiceModeState.speaking);
    await ttsProvider.speak(cleanText, onComplete: () {
      final current = ref.read(appControllerProvider);
      if (!current.isVoiceModeActive) return;
      if (current.voiceModeState == VoiceModeState.speaking) {
        controller.setVoiceModeState(VoiceModeState.idle);
      }
    });
  }

  Future<void> _enterVoiceMode() async {
    await ref.read(voicePreferencesProvider.notifier).ensureLoaded();
    final prefs = ref.read(voicePreferencesProvider);
    final provider = VoiceCatalog.provider(prefs.stt.providerId);
    final endpoint = prefs.stt.endpointOverride.isNotEmpty
        ? prefs.stt.endpointOverride
        : (provider.sttEndpoint ?? '');
    final model = prefs.stt.modelOverride.isNotEmpty
        ? prefs.stt.modelOverride
        : VoiceCatalog.sttModel(prefs.stt.providerId, prefs.stt.modelId).model;

    if (endpoint.isEmpty || model.isEmpty) {
      ref.read(appControllerProvider.notifier).setVoiceModeError(
            'Configure an STT provider endpoint and model in Voice settings.',
          );
      return;
    }

    if (provider.requiresApiKey) {
      final key = await ref
          .read(voicePreferencesProvider.notifier)
          .getApiKey(prefs.stt.providerId);
      if (key == null || key.trim().isEmpty) {
        ref.read(appControllerProvider.notifier).setVoiceModeError(
              'Add a ${provider.label} API key in Voice settings.',
            );
        return;
      }
    }

    if (!ref.read(appControllerProvider).isVoiceModeActive) return;
    if (isMobile) {
      await WakelockPlus.enable();
    }
    ref
        .read(appControllerProvider.notifier)
        .setVoiceModeState(VoiceModeState.idle);
  }

  Future<void> _exitVoiceMode() async {
    _voiceTimer?.cancel();
    _voiceTimer = null;
    _listeningStartedAt = null;
    if (mounted) setState(() => _elapsedSeconds = 0);
    await _recorder.cancel();
    await ref.read(ttsProviderResolver).stop();
    if (isMobile) {
      await WakelockPlus.disable();
    }
  }

  Future<void> _beginListening() async {
    final current = ref.read(appControllerProvider);
    if (!current.isVoiceModeActive || _voiceStarting) return;
    if (current.voiceModeState == VoiceModeState.listening) return;

    _voiceStarting = true;
    final controller = ref.read(appControllerProvider.notifier);
    try {
      await ref.read(ttsProviderResolver).stop();
      await _recorder.start();
      _listeningStartedAt = DateTime.now();
      if (mounted) setState(() => _elapsedSeconds = 0);
      controller.setVoiceModeState(VoiceModeState.listening);
      _voiceTimer?.cancel();
      _voiceTimer = Timer.periodic(const Duration(seconds: 1), (timer) {
        if (!mounted) return;
        final started = _listeningStartedAt;
        if (started == null) return;
        final seconds = DateTime.now().difference(started).inSeconds;
        setState(() => _elapsedSeconds = seconds);
        if (seconds >= 180) {
          timer.cancel();
          unawaited(_finishListening());
        }
      });
    } on VoiceRecorderException catch (e) {
      controller.setVoiceModeError(e.message);
    } catch (e) {
      controller.setVoiceModeError('Could not start listening: $e');
    } finally {
      _voiceStarting = false;
    }
  }

  Future<void> _finishListening() async {
    final current = ref.read(appControllerProvider);
    if (!current.isVoiceModeActive ||
        current.voiceModeState != VoiceModeState.listening) {
      return;
    }

    _voiceTimer?.cancel();
    _voiceTimer = null;
    final started = _listeningStartedAt;
    final duration = started == null
        ? Duration.zero
        : DateTime.now().difference(started);
    _listeningStartedAt = null;
    if (mounted) setState(() => _elapsedSeconds = 0);

    final path = await _recorder.stop();
    final controller = ref.read(appControllerProvider.notifier);

    if (path == null || duration < const Duration(milliseconds: 400)) {
      if (path != null) {
        try {
          await File(path).delete();
        } catch (_) {}
      }
      controller.setVoiceModeState(VoiceModeState.idle);
      return;
    }

    controller.setVoiceModeState(VoiceModeState.transcribing);
    try {
      final text = await ref.read(sttServiceProvider).transcribeFile(path);
      if (!ref.read(appControllerProvider).isVoiceModeActive) return;
      final trimmed = text.trim();
      if (trimmed.isEmpty) {
        controller.setVoiceModeError('No speech detected. Try again.');
        return;
      }
      controller.setVoiceModeState(VoiceModeState.thinking);
      controller.sendPrompt(trimmed);
    } on SttException catch (e) {
      controller.setVoiceModeError(e.message);
    } catch (e) {
      controller.setVoiceModeError('Transcription failed: $e');
    }
  }

  void _retryVoiceMode() {
    ref
        .read(appControllerProvider.notifier)
        .setVoiceModeState(VoiceModeState.idle);
  }

  void _openSettings() {
    Navigator.of(context).push(
      MaterialPageRoute<void>(builder: (_) => const SettingsScreen()),
    );
  }

  void _openVoiceSettings() => _openSettings();

  /// Full attach flow: pick file → size check → optional downscale → stage.
  Future<void> _pickAndAttachFile() async {
    final result = await FilePicker.platform.pickFiles(
      type: FileType.any,
      allowMultiple: false,
    );
    if (result == null || result.files.single.path == null) return;

    final originalFile = File(result.files.single.path!);
    final fileSize = await originalFile.length();

    if (!mounted) return;

    if (fileSize > kMaxFileSizeBytes) {
      final mb = (fileSize / (1024 * 1024)).toStringAsFixed(1);
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('File too large ($mb MB, max 50 MB)')),
      );
      return;
    }

    final ext = originalFile.path.split('.').last.toLowerCase();
    final isImage = kImageExtensions.contains(ext);
    File fileToUpload = originalFile;

    if (isImage && fileSize > kImageDownscaleThresholdBytes && mounted) {
      final originalMb = (fileSize / (1024 * 1024)).toStringAsFixed(1);
      final choice = await showDialog<String>(
        context: context,
        builder: (ctx) => AlertDialog(
          title: const Text('Send image as'),
          content: Text(
            'Original: $originalMb MB\n'
            'Downscaled: resize to ≤1024 px, JPEG 80 % quality',
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(ctx, 'original'),
              child: const Text('Original'),
            ),
            FilledButton(
              onPressed: () => Navigator.pop(ctx, 'downscale'),
              child: const Text('Downscale'),
            ),
          ],
        ),
      );
      if (choice == 'downscale') {
        fileToUpload = await _downscaleImage(originalFile);
      }
    }

    if (!mounted) return;
    await ref.read(appControllerProvider.notifier).attachFile(fileToUpload);
  }

  /// Resize [file] to fit within 1024 px on the longest edge, encoded as
  /// JPEG quality 80. Returns a temp file; falls back to [file] on error.
  Future<File> _downscaleImage(File file) async {
    try {
      final bytes = await file.readAsBytes();
      final original = await compute(img.decodeImage, bytes);
      if (original == null) return file;

      const maxDim = 1024;
      final resized = original.width >= original.height
          ? img.copyResize(original, width: maxDim)
          : img.copyResize(original, height: maxDim);

      final jpegBytes = img.encodeJpg(resized, quality: 80);
      final baseName =
          file.path.split('/').last.replaceAll(RegExp(r'\.\w+$'), '');
      final tempFile = File(
        '${Directory.systemTemp.path}/${baseName}_thumb.jpg',
      );
      await tempFile.writeAsBytes(jpegBytes);
      return tempFile;
    } catch (e) {
      debugPrint('Downscale failed ($e), using original');
      return file;
    }
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

    // Detect conversation switches/loads: snapshot history and request a
    // one-shot jumpTo so a long lazy ListView doesn't chase a moving extent.
    ref.listen<String?>(
      appControllerProvider.select((s) => s.activeConversation?.id),
      (previousId, currentId) {
        if (currentId == null || currentId == _renderedConversationId) return;
        _renderedConversationId = currentId;
        final fresh = ref.read(appControllerProvider).chatMessages;
        _markHistoricalMessages(fresh);
        _lastObservedMessageCount = fresh.length;
        _pendingJumpToBottom = true;
      },
    );

    // Auto-scroll only when the message list actually grows (new bubble or
    // streaming delta arriving). Previously this ran on every rebuild, which
    // animated against a moving extent on huge histories.
    final messageCount = state.chatMessages.length;
    final shouldScroll = _pendingJumpToBottom || messageCount > _lastObservedMessageCount;
    final shouldJump = _pendingJumpToBottom;
    _pendingJumpToBottom = false;
    _lastObservedMessageCount = messageCount;
    if (shouldScroll) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (!_scrollController.hasClients) return;
        final target = _scrollController.position.maxScrollExtent;
        if (shouldJump) {
          _scrollController.jumpTo(target);
        } else {
          _scrollController.animateTo(
            target,
            duration: const Duration(milliseconds: 200),
            curve: Curves.easeOut,
          );
        }
      });
    }

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
          onMemories: controller.openMemories,
          onMcpServers: controller.openMcpServers,
          onSettings: _openSettings,
        ),
        body: Column(
          children: [
            _TopBar(
              title: heading,
              connection: state.connection,
              streaming: state.streaming,
              onSettings: _openSettings,
              onRequestCompact: controller.requestCompactConversation,
              onRequestResumeAgent: controller.requestResumeAgent,
              isConversationInternal: state.activeConversation?.internal ?? false,
              hasActiveConversation: state.activeConversation != null,
              onToggleConversationTransient: () {
                final id = state.activeConversation?.id;
                if (id == null) return;
                controller.setConversationInternal(
                  id,
                  !(state.activeConversation?.internal ?? false),
                );
              },
              newChatInternal: state.newChatInternal,
              onToggleNewChatInternal: controller.toggleNewChatInternal,
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
                        animate: !_historicalMessageIds.contains(message.id),
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
              // Voice mode overlay
              if (state.isVoiceModeActive)
                VoiceModeOverlay(
                  state: state.voiceModeState,
                  inputMode: ref.watch(voicePreferencesProvider).inputMode,
                  elapsedSeconds: _elapsedSeconds,
                  errorMessage: state.voiceModeError,
                  onStartListening: _beginListening,
                  onStopListening: _finishListening,
                  onClose: controller.stopVoiceMode,
                  onRetry: _retryVoiceMode,
                  onOpenSettings: _openVoiceSettings,
                ),
            ],
          ),
        ),
        _Composer(
          controller: _controller,
          focusNode: _focusNode,
          attachedFiles: state.attachedFiles,
          onSend: () {
            final text = _controller.text;
            final hasFiles = state.attachedFiles.isNotEmpty;
            if (text.trim().isNotEmpty || hasFiles) {
              _focusNode.unfocus();
              _sentPlayer.stop();
              unawaited(_sentPlayer.play(AssetSource('audio/sent.mp3')));
              controller.sendPrompt(text);
              _controller.clear();
            }
          },
          onVoiceMode: () {
            controller.startVoiceMode();
          },
          onAttachFile: _pickAndAttachFile,
          onRemoveFile: (fileId) {
            controller.removeAttachedFile(fileId);
          },
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
    this.onRequestCompact,
    this.onRequestResumeAgent,
    this.isConversationInternal = false,
    this.hasActiveConversation = false,
    this.onToggleConversationTransient,
    this.newChatInternal = false,
    this.onToggleNewChatInternal,
  });

  final String title;
  final ConnectionStatus connection;
  final bool streaming;
  final VoidCallback onSettings;
  final VoidCallback? onRequestCompact;
  final VoidCallback? onRequestResumeAgent;
  final bool isConversationInternal;
  final bool hasActiveConversation;
  final VoidCallback? onToggleConversationTransient;
  final bool newChatInternal;
  final VoidCallback? onToggleNewChatInternal;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final connectionIcon = switch (connection) {
      ConnectionStatus.connecting => Icons.sync,
      ConnectionStatus.online => Icons.circle,
      ConnectionStatus.error => Icons.error,
    };
    final connectionColor = switch (connection) {
      ConnectionStatus.connecting => Theme.of(context).colorScheme.warning,
      ConnectionStatus.online => Theme.of(context).colorScheme.success,
      ConnectionStatus.error => Theme.of(context).colorScheme.error,
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
          _StatusMenuAnchor(
            connectionIcon: connectionIcon,
            connectionColor: connectionColor,
            streaming: streaming,
            onRequestCompact: onRequestCompact,
            onRequestResumeAgent: onRequestResumeAgent,
            isConversationInternal: isConversationInternal,
            hasActiveConversation: hasActiveConversation,
            onToggleConversationTransient: onToggleConversationTransient,
            newChatInternal: newChatInternal,
            onToggleNewChatInternal: onToggleNewChatInternal,
          ),
        ],
      ),
    );
  }
}

/// Tappable status icon that shows "Current chat" menu with Compact option.
class _StatusMenuAnchor extends StatelessWidget {
  const _StatusMenuAnchor({
    required this.connectionIcon,
    required this.connectionColor,
    required this.streaming,
    this.onRequestCompact,
    this.onRequestResumeAgent,
    this.isConversationInternal = false,
    this.hasActiveConversation = false,
    this.onToggleConversationTransient,
    this.newChatInternal = false,
    this.onToggleNewChatInternal,
  });

  final IconData connectionIcon;
  final Color connectionColor;
  final bool streaming;
  final VoidCallback? onRequestCompact;
  final VoidCallback? onRequestResumeAgent;
  final bool isConversationInternal;
  final bool hasActiveConversation;
  final VoidCallback? onToggleConversationTransient;
  final bool newChatInternal;
  final VoidCallback? onToggleNewChatInternal;

  @override
  Widget build(BuildContext context) {
    return Material(
      color: Colors.transparent,
      child: InkWell(
        borderRadius: BorderRadius.circular(20),
        onTap: () => _showCurrentChatMenu(context),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 8),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(connectionIcon, color: connectionColor, size: 16),
              if (streaming)
                const Padding(
                  padding: EdgeInsets.only(left: 8),
                  child: Text('Streaming…', style: TextStyle(fontSize: 12)),
                ),
            ],
          ),
        ),
      ),
    );
  }

  void _showCurrentChatMenu(BuildContext context) {
    final box = context.findRenderObject() as RenderBox?;
    final offset = box != null ? box.localToGlobal(Offset.zero) : Offset.zero;
    final size = box != null ? box.size : Size.zero;

    showMenu<String>(
      context: context,
      position: RelativeRect.fromLTRB(
        offset.dx,
        offset.dy + size.height,
        offset.dx + size.width,
        offset.dy + size.height + 8,
      ),
      items: [
        PopupMenuItem<String>(
          enabled: false,
          child: Text(
            'Current chat',
            style: TextStyle(
              fontWeight: FontWeight.w600,
              color: Theme.of(context).colorScheme.onSurface.withValues(alpha: 0.7),
            ),
          ),
        ),
        const PopupMenuItem<String>(
          value: 'compact',
          child: ListTile(
            contentPadding: EdgeInsets.zero,
            leading: Icon(Icons.compress),
            title: Text('Compact'),
          ),
        ),
        PopupMenuItem<String>(
          value: 'resume',
          enabled: !streaming,
          child: ListTile(
            contentPadding: EdgeInsets.zero,
            leading: Icon(
              Icons.play_arrow,
              color: streaming
                  ? Theme.of(context).disabledColor
                  : null,
            ),
            title: const Text('Resume agent'),
          ),
        ),
        if (hasActiveConversation)
          PopupMenuItem<String>(
            value: 'transient',
            child: ListTile(
              contentPadding: EdgeInsets.zero,
              leading: Icon(
                isConversationInternal
                    ? Icons.visibility_outlined
                    : Icons.visibility_off_outlined,
              ),
              title: Text(
                isConversationInternal ? 'Remove transient' : 'Mark transient',
              ),
            ),
          ),
        PopupMenuItem<String>(
          value: 'new_chat_transient',
          child: ListTile(
            contentPadding: EdgeInsets.zero,
            leading: Icon(
              newChatInternal ? Icons.check_box : Icons.check_box_outline_blank,
            ),
            title: Text(
              newChatInternal ? 'New chats: transient' : 'New chats: normal',
            ),
          ),
        ),
      ],
    ).then((value) {
      if (!context.mounted) return;
      if (value == 'compact') {
        onRequestCompact?.call();
      } else if (value == 'resume') {
        onRequestResumeAgent?.call();
      } else if (value == 'transient') {
        onToggleConversationTransient?.call();
      } else if (value == 'new_chat_transient') {
        onToggleNewChatInternal?.call();
      }
    });
  }
}

class _ChatDrawer extends ConsumerWidget {
  const _ChatDrawer({
    required this.profile,
    required this.availableProfiles,
    required this.onProfileChanged,
    required this.onStartNew,
    required this.onHistory,
    required this.onMemories,
    required this.onMcpServers,
    required this.onSettings,
  });

  final String profile;
  final List<String> availableProfiles;
  final Function(String) onProfileChanged;
  final VoidCallback onStartNew;
  final VoidCallback onHistory;
  final VoidCallback onMemories;
  final VoidCallback onMcpServers;
  final VoidCallback onSettings;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final voicePrefs = ref.watch(voicePreferencesProvider);
    final voiceNotifier = ref.read(voicePreferencesProvider.notifier);
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
                        color:
                            Theme.of(context).colorScheme.onPrimaryContainer,
                      ),
                ),
              ],
            ),
          ),
          ListTile(
            leading: const Icon(Icons.add),
            title: const Text('New Chat'),
            onTap: () {
              Navigator.pop(context);
              onStartNew();
            },
          ),
          SwitchListTile(
            secondary: const Icon(Icons.volume_up),
            title: const Text('Text-to-Speech'),
            subtitle: const Text('Read assistant replies aloud'),
            value: voicePrefs.ttsEnabled,
            onChanged: (value) {
              voiceNotifier.setTtsEnabled(value);
            },
          ),
          const Divider(),
          if (availableProfiles.isNotEmpty)
            ExpansionTile(
              leading: const Icon(Icons.person),
              title: const Text('Profile'),
              subtitle: Text(profile),
              children: availableProfiles.map((p) {
                return ListTile(
                  title: Text(p),
                  selected: p == profile,
                  onTap: () {
                    Navigator.pop(context);
                    onProfileChanged(p);
                  },
                );
              }).toList(),
            ),
          ListTile(
            leading: const Icon(Icons.history),
            title: const Text('History'),
            onTap: () {
              Navigator.pop(context);
              onHistory();
            },
          ),
          ListTile(
            leading: const Icon(Icons.psychology_outlined),
            title: const Text('Memories'),
            onTap: () {
              Navigator.pop(context);
              onMemories();
            },
          ),
          ListTile(
            leading: const Icon(Icons.hub_outlined),
            title: const Text('MCP Servers'),
            onTap: () {
              Navigator.pop(context);
              onMcpServers();
            },
          ),
          const Divider(),
          ListTile(
            leading: const Icon(Icons.settings),
            title: const Text('Settings'),
            onTap: () {
              Navigator.pop(context);
              onSettings();
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
    final hasFiles = widget.attachedFiles.isNotEmpty;
    final canSend = hasText || hasFiles;

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
                color: colorScheme.onSurfaceVariant.withValues(alpha: 0.6),
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
                  if (!isStreaming && !state.isVoiceModeActive)
                    IconButton(
                      onPressed: widget.onAttachFile,
                      icon: const Icon(Icons.add),
                      tooltip: 'Attach file',
                    ),
                  // Voice mode button (mobile only)
                  if (isMobile && !isStreaming && !state.isVoiceModeActive)
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
              else if (canSend)
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
