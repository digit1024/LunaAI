import 'dart:async';

import 'package:audioplayers/audioplayers.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:wakelock_plus/wakelock_plus.dart';

import 'package:luna_mobile/application/app_controller.dart';
import 'package:luna_mobile/application/app_state.dart';
import 'package:luna_mobile/core/config/stt_preferences.dart';
import 'package:luna_mobile/core/config/tts_preferences.dart';
import 'package:luna_mobile/services/tts_service.dart';
import 'package:luna_mobile/tts/message_speech.dart';
import 'package:luna_mobile/utils/platform_utils.dart';
import 'widgets/wear_message_bubble.dart';
import 'widgets/wear_voice_overlay.dart';

class WearChatScreen extends ConsumerStatefulWidget {
  const WearChatScreen({super.key});

  @override
  ConsumerState<WearChatScreen> createState() => _WearChatScreenState();
}

class _WearChatScreenState extends ConsumerState<WearChatScreen> {
  static const _speechChannel = MethodChannel('com.luna.mobile.wear/speech');

  final _scrollController = ScrollController();
  late final AudioPlayer _sentPlayer;
  late final AudioPlayer _donePlayer;
  late final ProviderSubscription<bool> _streamingSubscription;
  late final ProviderSubscription<List<ChatMessage>> _ttsSubscription;
  final Set<String> _processedMessageIds = {};
  bool _isRecognizing = false;

  // Dialog mode — continuous voice loop
  bool _isDialogMode = false;
  DialogModeState _dialogModeState = DialogModeState.listening;

  @override
  void initState() {
    super.initState();
    _sentPlayer = AudioPlayer(playerId: 'wear_sent')
      ..setReleaseMode(ReleaseMode.stop);
    _donePlayer = AudioPlayer(playerId: 'wear_done')
      ..setReleaseMode(ReleaseMode.stop);
    unawaited(_sentPlayer.setSource(AssetSource('audio/sent.mp3')));
    unawaited(_donePlayer.setSource(AssetSource('audio/done.mp3')));

    final streamingProvider =
        appControllerProvider.select((state) => state.streaming);
    _streamingSubscription = ref.listenManual<bool>(
      streamingProvider,
      (previous, next) {
        if (!next && previous == true) {
          // Streaming just finished
          if (_isDialogMode) {
            setState(() => _dialogModeState = DialogModeState.speaking);
          }
          _donePlayer.stop();
          unawaited(_donePlayer.play(AssetSource('audio/done.mp3')).then((_) {
            Future.delayed(const Duration(milliseconds: 300), () {
              _triggerTtsForLastMessage();
            });
          }));
        }
      },
    );

    _ttsSubscription = ref.listenManual<List<ChatMessage>>(
      appControllerProvider.select((state) => state.chatMessages),
      (previous, next) {
        _handleNewAssistantMessages(previous, next);
      },
    );
  }

  @override
  void dispose() {
    _streamingSubscription.close();
    _ttsSubscription.close();
    _sentPlayer.dispose();
    _donePlayer.dispose();
    _scrollController.dispose();
    super.dispose();
  }

  void _handleNewAssistantMessages(
    List<ChatMessage>? previous,
    List<ChatMessage> next,
  ) {
    final ttsPrefs = ref.read(ttsPreferencesProvider);
    if (!ttsPrefs.enabled) return;

    final completedMessages = next.where(
      (m) => m.bubbleType == BubbleType.assistant && !m.isStreaming,
    ).where((m) {
      final wasStreaming = previous?.any((p) => p.id == m.id && p.isStreaming) ?? false;
      return wasStreaming;
    }).toList();

    for (final message in completedMessages) {
      if (_processedMessageIds.contains('${message.id}_tts')) continue;
      _processedMessageIds.add('${message.id}_tts');
      _playTtsForMessage(message);
    }
  }

  Future<void> _playTtsForMessage(ChatMessage message) async {
    if (message.content.isEmpty) return;

    final ttsPrefs = ref.read(ttsPreferencesProvider);
    final cleanText = prepareMessageForTts(message.content);
    if (cleanText.trim().isEmpty) return;

    final ttsService = ref.read(ttsServiceProvider);
    await ttsService.setLanguage(ttsPrefs.language);
    await ttsService.stop();

    if (_isDialogMode) {
      await ttsService.speak(cleanText, onComplete: () {
        if (mounted && _isDialogMode) {
          setState(() => _dialogModeState = DialogModeState.listening);
          _startVoiceInput(dialogMode: true);
        }
      });
    } else {
      await ttsService.speak(cleanText);
    }
  }

  Future<void> _triggerTtsForLastMessage() async {
    final state = ref.read(appControllerProvider);
    final ttsPrefs = ref.read(ttsPreferencesProvider);

    if (!ttsPrefs.enabled) return;
    if (state.chatMessages.isEmpty) return;

    final lastAssistant = state.chatMessages.lastWhere(
      (m) => m.bubbleType == BubbleType.assistant && !m.isStreaming,
      orElse: () => ChatMessage(
        id: '',
        role: 'assistant',
        content: '',
        timestamp: DateTime.now(),
        bubbleType: BubbleType.assistant,
      ),
    );

    if (lastAssistant.content.isEmpty) return;
    if (_processedMessageIds.contains(lastAssistant.id)) return;
    _processedMessageIds.add(lastAssistant.id);

    await _playTtsForMessage(lastAssistant);
  }

  // ── Dialog Mode ────────────────────────────────────────────────────────────

  void _enterDialogMode() {
    setState(() {
      _isDialogMode = true;
      _dialogModeState = DialogModeState.listening;
    });
    _startVoiceInput(dialogMode: true);
  }

  void _exitDialogMode() {
    ref.read(ttsServiceProvider).stop();
    setState(() {
      _isDialogMode = false;
      _isRecognizing = false;
    });
  }

  // ── Voice Input ────────────────────────────────────────────────────────────

  Future<void> _startVoiceInput({bool dialogMode = false}) async {
    if (_isRecognizing) return;

    final ttsService = ref.read(ttsServiceProvider);
    await ttsService.stop();

    setState(() => _isRecognizing = true);

    if (isMobile) await WakelockPlus.enable();

    try {
      final sttPrefs = ref.read(sttPreferencesProvider);

      final result = await _speechChannel.invokeMethod<Map<dynamic, dynamic>>(
        'startSpeechRecognition',
        {
          'language': sttPrefs.language,
          'prompt': 'Speak to Luna',
        },
      );

      if (result != null) {
        final success = result['success'] as bool? ?? false;
        final text = result['text'] as String? ?? '';

        if (success && text.isNotEmpty) {
          _sentPlayer.stop();
          unawaited(_sentPlayer.play(AssetSource('audio/sent.mp3')));

          if (dialogMode && _isDialogMode) {
            setState(() => _dialogModeState = DialogModeState.processing);
          }

          final controller = ref.read(appControllerProvider.notifier);
          controller.sendPrompt(text);
        } else if (dialogMode && _isDialogMode) {
          // No speech detected — loop back to listening
          setState(() => _dialogModeState = DialogModeState.listening);
          _startVoiceInput(dialogMode: true);
        }
      }
    } on PlatformException catch (e) {
      debugPrint('Speech recognition error: ${e.message}');
      if (dialogMode && mounted && _isDialogMode) {
        _exitDialogMode();
      } else if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text('Voice error: ${e.message}'),
            duration: const Duration(seconds: 2),
          ),
        );
      }
    } on MissingPluginException {
      debugPrint('Speech plugin not available');
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(
            content: Text('Voice input not available'),
            duration: Duration(seconds: 2),
          ),
        );
      }
    } finally {
      if (isMobile) await WakelockPlus.disable();
      if (mounted) setState(() => _isRecognizing = false);
    }
  }

  String _getLanguageDisplayName(String languageCode) {
    final parts = languageCode.split('-');
    final lang = parts[0];
    final country = parts.length > 1 ? parts[1] : null;

    const languageNames = {
      'en': 'English', 'es': 'Spanish', 'fr': 'French', 'de': 'German',
      'it': 'Italian', 'pt': 'Portuguese', 'ru': 'Russian', 'ja': 'Japanese',
      'ko': 'Korean', 'zh': 'Chinese', 'ar': 'Arabic', 'hi': 'Hindi',
      'nl': 'Dutch', 'pl': 'Polish', 'tr': 'Turkish', 'uk': 'Ukrainian',
    };

    final langName = languageNames[lang] ?? lang.toUpperCase();
    return country != null ? '$langName ($country)' : langName;
  }

  void _showOptionsMenu() {
    final controller = ref.read(appControllerProvider.notifier);

    showModalBottomSheet(
      context: context,
      backgroundColor: Theme.of(context).colorScheme.surface,
      isScrollControlled: true,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(16)),
      ),
      builder: (context) => Consumer(
        builder: (context, ref, _) {
          final currentTtsPrefs = ref.watch(ttsPreferencesProvider);
          final currentSttPrefs = ref.watch(sttPreferencesProvider);
          final currentState = ref.watch(appControllerProvider);

          return SafeArea(
            child: SingleChildScrollView(
              child: Padding(
                padding: const EdgeInsets.symmetric(vertical: 8),
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    ListTile(
                      leading: const Icon(Icons.add, size: 20),
                      title: const Text('New Chat', style: TextStyle(fontSize: 14)),
                      dense: true,
                      onTap: () {
                        Navigator.pop(context);
                        controller.startNewConversation();
                      },
                    ),
                    const Divider(height: 1),
                    if (currentState.availableProfiles.isNotEmpty) ...[
                      ListTile(
                        leading: const Icon(Icons.person, size: 20),
                        title: const Text('Profile', style: TextStyle(fontSize: 14)),
                        trailing: DropdownButton<String>(
                          value: currentState.currentProfile.isNotEmpty
                              ? currentState.currentProfile
                              : currentState.availableProfiles.first,
                          isDense: true,
                          underline: const SizedBox(),
                          style: TextStyle(
                            fontSize: 12,
                            color: Theme.of(context).colorScheme.primary,
                          ),
                          items: currentState.availableProfiles.map((profile) {
                            return DropdownMenuItem(
                              value: profile,
                              child: Text(profile, style: const TextStyle(fontSize: 12)),
                            );
                          }).toList(),
                          onChanged: (value) {
                            if (value != null) controller.changeProfile(value);
                          },
                        ),
                        dense: true,
                      ),
                      const Divider(height: 1),
                    ],
                    SwitchListTile(
                      secondary: const Icon(Icons.volume_up, size: 20),
                      title: const Text('TTS', style: TextStyle(fontSize: 14)),
                      value: currentTtsPrefs.enabled,
                      dense: true,
                      onChanged: (value) {
                        ref.read(ttsPreferencesProvider.notifier).setEnabled(value);
                        if (!value) ref.read(ttsServiceProvider).stop();
                      },
                    ),
                    const Divider(height: 1),
                    ListTile(
                      leading: const Icon(Icons.language, size: 20),
                      title: const Text('Voice Language', style: TextStyle(fontSize: 14)),
                      trailing: DropdownButton<String>(
                        value: currentSttPrefs.language,
                        isDense: true,
                        underline: const SizedBox(),
                        style: TextStyle(
                          fontSize: 12,
                          color: Theme.of(context).colorScheme.primary,
                        ),
                        items: currentSttPrefs.favoriteLanguages.map((lang) {
                          return DropdownMenuItem(
                            value: lang,
                            child: Text(
                              _getLanguageDisplayName(lang),
                              style: const TextStyle(fontSize: 12),
                            ),
                          );
                        }).toList(),
                        onChanged: (value) {
                          if (value != null) {
                            ref.read(sttPreferencesProvider.notifier).setLanguage(value);
                            ref.read(ttsPreferencesProvider.notifier).setLanguage(value);
                          }
                        },
                      ),
                      dense: true,
                    ),
                    const Divider(height: 1),
                    ListTile(
                      leading: const Icon(Icons.history, size: 20),
                      title: const Text('History', style: TextStyle(fontSize: 14)),
                      dense: true,
                      onTap: () {
                        Navigator.pop(context);
                        controller.openConversations();
                      },
                    ),
                    const Divider(height: 1),
                    ListTile(
                      leading: const Icon(Icons.settings, size: 20),
                      title: const Text('Settings', style: TextStyle(fontSize: 14)),
                      dense: true,
                      onTap: () {
                        Navigator.pop(context);
                        controller.openSetup();
                      },
                    ),
                  ],
                ),
              ),
            ),
          );
        },
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final state = ref.watch(appControllerProvider);
    final controller = ref.read(appControllerProvider.notifier);

    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (_scrollController.hasClients) {
        _scrollController.animateTo(
          _scrollController.position.maxScrollExtent,
          duration: const Duration(milliseconds: 200),
          curve: Curves.easeOut,
        );
      }
    });

    final connectionColor = switch (state.connection) {
      ConnectionStatus.connecting => Colors.orange,
      ConnectionStatus.online => Colors.green,
      ConnectionStatus.error => Colors.red,
    };

    return Scaffold(
      body: Stack(
        children: [
          // ── Main chat UI ──────────────────────────────────────────────────
          Column(
            children: [
              Expanded(
                child: state.chatMessages.isEmpty
                    ? Center(
                        child: Column(
                          mainAxisAlignment: MainAxisAlignment.center,
                          children: [
                            Icon(
                              Icons.chat_bubble_outline,
                              size: 32,
                              color: Theme.of(context)
                                  .colorScheme
                                  .onSurface
                                  .withValues(alpha: 0.5),
                            ),
                            const SizedBox(height: 8),
                            Text(
                              'Tap mic · Hold for dialog',
                              style: TextStyle(
                                fontSize: 11,
                                color: Theme.of(context)
                                    .colorScheme
                                    .onSurface
                                    .withValues(alpha: 0.5),
                              ),
                            ),
                          ],
                        ),
                      )
                    : ListView.builder(
                        controller: _scrollController,
                        padding: const EdgeInsets.fromLTRB(12, 24, 12, 8),
                        itemCount: state.chatMessages.length +
                            (state.streaming ? 1 : 0),
                        itemBuilder: (context, index) {
                          if (index < state.chatMessages.length) {
                            final message = state.chatMessages[index];
                            return Padding(
                              padding:
                                  const EdgeInsets.symmetric(vertical: 4),
                              child: WearMessageBubble(message: message),
                            );
                          }
                          return const Padding(
                            padding: EdgeInsets.symmetric(vertical: 4),
                            child: Center(
                              child: SizedBox(
                                width: 20,
                                height: 20,
                                child: CircularProgressIndicator(
                                    strokeWidth: 2),
                              ),
                            ),
                          );
                        },
                      ),
              ),
              // Bottom action bar
              Container(
                padding: const EdgeInsets.fromLTRB(16, 4, 16, 16),
                child: Row(
                  mainAxisAlignment: MainAxisAlignment.spaceEvenly,
                  children: [
                    Stack(
                      children: [
                        IconButton(
                          icon: const Icon(Icons.more_horiz, size: 24),
                          onPressed: _showOptionsMenu,
                          tooltip: 'Options',
                        ),
                        Positioned(
                          right: 8,
                          top: 8,
                          child: Container(
                            width: 8,
                            height: 8,
                            decoration: BoxDecoration(
                              color: connectionColor,
                              shape: BoxShape.circle,
                            ),
                          ),
                        ),
                      ],
                    ),
                    if (state.streaming)
                      SizedBox(
                        width: 56,
                        height: 56,
                        child: FloatingActionButton(
                          onPressed: () => controller.stopStreaming(
                            conversationId: state.activeConversation?.id,
                          ),
                          backgroundColor: Colors.red,
                          child: const Icon(Icons.stop, size: 28),
                        ),
                      )
                    else
                      SizedBox(
                        width: 56,
                        height: 56,
                        child: GestureDetector(
                          onLongPress: _isRecognizing ? null : _enterDialogMode,
                          child: FloatingActionButton(
                            onPressed: _isRecognizing
                                ? null
                                : () => _startVoiceInput(),
                            backgroundColor: _isRecognizing
                                ? Colors.grey
                                : Theme.of(context).colorScheme.primary,
                            tooltip: 'Tap: one-shot · Hold: dialog loop',
                            child: Icon(
                              _isRecognizing ? Icons.mic_off : Icons.mic,
                              size: 28,
                            ),
                          ),
                        ),
                      ),
                  ],
                ),
              ),
            ],
          ),

          // ── One-shot listening indicator (non-dialog mode) ────────────────
          if (_isRecognizing && !_isDialogMode)
            Container(
              color: Colors.black54,
              child: const Center(
                child: Column(
                  mainAxisAlignment: MainAxisAlignment.center,
                  children: [
                    CircularProgressIndicator(),
                    SizedBox(height: 16),
                    Text(
                      'Listening...',
                      style: TextStyle(color: Colors.white),
                    ),
                  ],
                ),
              ),
            ),

          // ── Dialog mode voice overlay ─────────────────────────────────────
          if (_isDialogMode)
            Positioned.fill(
              child: WearVoiceOverlay(
                state: _dialogModeState,
                onClose: _exitDialogMode,
                onStopTts: () {
                  ref.read(ttsServiceProvider).stop();
                  if (mounted && _isDialogMode) {
                    setState(() => _dialogModeState = DialogModeState.listening);
                    _startVoiceInput(dialogMode: true);
                  }
                },
              ),
            ),
        ],
      ),
    );
  }
}
