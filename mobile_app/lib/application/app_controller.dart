import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../core/config/server_config.dart';
import '../data/http/file_client.dart';
import '../data/ws/luna_ws_client.dart';
import '../data/ws/ws_dto.dart';
import '../services/foreground_guard.dart';
import '../services/notification_service.dart';
import '../utils/platform_utils.dart';
import 'app_state.dart';

final notificationServiceProvider = Provider<NotificationService>((_) {
  throw UnimplementedError('notificationServiceProvider must be overridden');
});

final foregroundGuardProvider = Provider<ForegroundGuard>((_) {
  throw UnimplementedError('foregroundGuardProvider must be overridden');
});

/// Singleton WebSocket client - does NOT recreate on config changes!
/// Config is read at connect time, not at provider creation.
final wsClientProvider = Provider<LunaWsClient>((ref) {
  final client = LunaWsClient();
  ref.onDispose(() => client.dispose());
  return client;
});

/// Singleton AppController - does NOT recreate on config/wsClient changes!
final appControllerProvider =
    NotifierProvider<AppController, AppState>(AppController.new);

class AppController extends Notifier<AppState> {
  late final LunaWsClient _wsClient;
  late final NotificationService _notifications;
  late final ForegroundGuard _guard;

  @override
  AppState build() {
    // Use ref.read to avoid recreation on changes
    _wsClient = ref.read(wsClientProvider);
    _notifications = ref.read(notificationServiceProvider);
    _guard = ref.read(foregroundGuardProvider);
    
    // Initialize and auto-connect
    Future.microtask(() async {
      await init();
    });
    
    // Setup disposal
    ref.onDispose(() {
      unawaited(_subscription?.cancel());
      unawaited(_guard.stopConnectionGuard());
      unawaited(_wsClient.dispose());
    });
    
    return AppState.initial();
  }

  LunaWsClient get wsClient => _wsClient;
  NotificationService get notifications => _notifications;
  ForegroundGuard get guard => _guard;

  StreamSubscription<ServerEvent>? _subscription;
  bool _initialized = false;
  bool _waitingForResponse = false;
  bool _connecting = false;
  
  /// Tracks current assistant bubble ID for streaming
  /// Reset when tools interrupt or new turn starts
  String? _currentAssistantBubbleId;

  static const int _maxConnectionAttempts = 3;
  static const Duration _connectionRetryDelay = Duration(seconds: 2);

  Future<void> init() async {
    if (_initialized) return;
    _initialized = true;
    await notifications.init();
    await guard.init();
    // Wait for saved config to load before auto-connecting
    await ref.read(serverConfigProvider.notifier).ensureLoaded();
    // Auto-connect on startup
    unawaited(connect());
  }

  /// Attempts to connect up to [_maxConnectionAttempts] times.
  /// Shows "connecting" screen with attempt count.
  /// Falls back to setup screen if all attempts fail.
  /// 
  /// [silent] - If true, preserves current pane and doesn't show connecting screen.
  ///            Used for background reconnections to avoid disrupting user experience.
  Future<void> connect({bool silent = false}) async {
    if (_connecting) return; // Prevent concurrent connect calls
    _connecting = true;

    // Cancel any existing subscription to prevent duplicates
    await _subscription?.cancel();
    _subscription = null;

    final config = ref.read(serverConfigProvider);
    final currentPane = state.pane; // Preserve current pane for silent mode
    final wasInChat = state.pane == ActivePane.chat;
    final activeConversationId = state.activeConversation?.id; // Preserve active conversation

    for (int attempt = 1; attempt <= _maxConnectionAttempts; attempt++) {
      // Only change pane to connecting if not in silent mode and not already on a meaningful screen
      if (!silent && (currentPane == ActivePane.setup || currentPane == ActivePane.connecting)) {
        state = state.copyWith(
          pane: ActivePane.connecting,
          error: null,
          connectionAttempt: attempt,
        );
      } else if (!silent) {
        // Show connection attempt but keep current pane
        state = state.copyWith(
          error: null,
          connectionAttempt: attempt,
        );
      } else {
        // Silent mode - only update connection attempt internally, don't change UI
        state = state.copyWith(
          error: null,
          connectionAttempt: attempt,
        );
      }

      try {
        await wsClient.connect(config);

        // Listen for events
        _subscription = wsClient.events.listen(_handleEvent);

        // Send initial handshake commands
        wsClient.send(ClientCommand.healthCheck());
        wsClient.send(ClientCommand.listConversations());
        wsClient.send(ClientCommand.listProfiles());

        _connecting = false;
        return; // Success!
      } catch (e) {
        debugPrint('Connection attempt $attempt failed: $e');

        if (attempt < _maxConnectionAttempts) {
          // Wait before retry
          await Future<void>.delayed(_connectionRetryDelay);
        }
      }
    }

    // All attempts failed
    _connecting = false;
    
    // Only change pane if not in silent mode or if we were on setup/connecting
    if (!silent || currentPane == ActivePane.setup || currentPane == ActivePane.connecting) {
      state = state.copyWith(
        pane: ActivePane.setup,
        connection: ConnectionStatus.error,
        error: 'Could not connect after $_maxConnectionAttempts attempts.\n'
            'Please check server settings and try again.',
        connectionAttempt: 0,
      );
    } else {
      // Silent mode failure - preserve pane but show error
      state = state.copyWith(
        connection: ConnectionStatus.error,
        error: 'Connection lost. Please check your connection.',
        connectionAttempt: 0,
      );
    }
  }


  void refreshConversations() {
    wsClient.send(ClientCommand.listConversations(limit: 10));
  }

  void loadMoreConversations(int offset) {
    wsClient.send(ClientCommand.listConversations(offset: offset, limit: 10));
  }

  void deleteConversation(String conversationId) {
    wsClient.send(ClientCommand.deleteConversation(conversationId));
  }

  void stopStreaming({String? conversationId}) {
    wsClient.send(ClientCommand.stopStreaming(conversationId: conversationId));
  }

  void search(String query) {
    final trimmed = query.trim();
    if (trimmed.isEmpty) {
      state = state.copyWith(searchQuery: '', searchResults: []);
      refreshConversations();
    } else {
      state = state.copyWith(searchQuery: trimmed);
      wsClient.send(ClientCommand.search(trimmed));
    }
  }

  void openConversations() {
    state = state.copyWith(pane: ActivePane.conversations);
    refreshConversations();
  }

  void openSetup() {
    state = state.copyWith(pane: ActivePane.setup);
  }

  void openSettings() {
    state = state.copyWith(pane: ActivePane.setup);
  }

  void selectConversation(String id) {
    wsClient.send(ClientCommand.loadConversation(id));
    state = state.copyWith(pane: ActivePane.chat);
  }

  void setBackgrounded(bool value) {
    state = state.copyWith(backgrounded: value);
  }

  /// Check connection health and reconnect if needed
  /// Called when app resumes from background
  /// Uses silent reconnection to preserve UI state
  Future<void> checkAndReconnect() async {
    // Check both WebSocket client state and app state
    // Connection might be dead even if channel exists (e.g., OS closed it)
    final isConnected = wsClient.isConnected && 
                       state.connection == ConnectionStatus.online;
    
    if (isConnected) {
      // Verify connection is actually alive with a health check
      wsClient.send(ClientCommand.healthCheck());
      // Ensure guard is running to keep connection alive
      unawaited(guard.startConnectionGuard());
      // Connection appears alive, no need to reconnect
      return;
    }
    
    // Connection appears dead - attempt silent reconnection
    // This preserves the current pane and doesn't disrupt the user experience
    debugPrint('Connection lost, attempting silent reconnect...');
    final wasInChat = state.pane == ActivePane.chat;
    final activeConversationId = state.activeConversation?.id;
    
    await connect(silent: true);
    
    // After silent reconnect, if we were in a chat, reload that conversation
    // but only if we don't already have it loaded
    if (wasInChat && activeConversationId != null) {
      // Check if we still have the conversation loaded
      if (state.activeConversation?.id != activeConversationId) {
        // Conversation was lost, reload it
        debugPrint('Reloading conversation after reconnect: $activeConversationId');
        wsClient.send(ClientCommand.loadConversation(activeConversationId));
      }
    }
  }

  Future<void> startNewConversation() async {
    // Ensure profile is set before starting a new conversation
    _ensureProfileIsSet(null);
    wsClient.send(ClientCommand.startConversation('Generating title...'));
  }

  Future<void> changeProfile(String profile) async {
    wsClient.send(ClientCommand.changeProfile(profile));
  }

  Future<void> sendPrompt(String text) async {
    if (text.trim().isEmpty && state.attachedFiles.isEmpty) return;
    final conversationId = state.activeConversation?.id;
    final attachmentIds = state.attachedFiles.map((f) => f.fileId).toList();
    _appendUserMessage(text.trim());
    await guard.ensureStarted('Streaming reply');
    _waitingForResponse = true;
    wsClient.send(ClientCommand.sendMessage(
      conversationId: conversationId,
      content: text.trim(),
      attachmentIds: attachmentIds.isNotEmpty ? attachmentIds : null,
    ));
    // Clear attached files after sending
    if (attachmentIds.isNotEmpty) {
      state = state.copyWith(attachedFiles: []);
    }
  }

  void startDialogMode() {
    // Voice mode only available on mobile platforms
    if (!isMobile) {
      debugPrint('Dialog mode not available on desktop/web platform');
      state = state.copyWith(
        error: 'Voice mode is only available on mobile devices',
      );
      return;
    }
    
    if (state.isDialogModeActive) return;
    state = state.copyWith(
      isDialogModeActive: true,
      dialogModeState: DialogModeState.listening,
    );
  }

  /// Attach a file from a File object
  Future<void> attachFile(File file) async {
    try {
      final config = ref.read(serverConfigProvider);
      final fileClient = FileClient(config);
      final attachment = await fileClient.uploadFile(file);
      state = state.copyWith(
        attachedFiles: [...state.attachedFiles, attachment],
      );
    } catch (e) {
      debugPrint('Error attaching file: $e');
      state = state.copyWith(
        error: 'Failed to attach file: ${e.toString()}',
      );
    }
  }

  /// Remove an attached file
  Future<void> removeAttachedFile(String fileId) async {
    try {
      final config = ref.read(serverConfigProvider);
      final fileClient = FileClient(config);
      await fileClient.removeFile(fileId);
      state = state.copyWith(
        attachedFiles: state.attachedFiles
            .where((f) => f.fileId != fileId)
            .toList(),
      );
    } catch (e) {
      debugPrint('Error removing file: $e');
      // Still remove from UI even if server removal fails
      state = state.copyWith(
        attachedFiles: state.attachedFiles
            .where((f) => f.fileId != fileId)
            .toList(),
      );
    }
  }

  void stopDialogMode() {
    if (!state.isDialogModeActive) return;
    state = state.copyWith(
      isDialogModeActive: false,
      dialogModeState: DialogModeState.listening,
    );
  }

  void setDialogModeState(DialogModeState newState) {
    if (!state.isDialogModeActive) return;
    state = state.copyWith(dialogModeState: newState);
  }

  /// Ensures the saved profile from config is set on the server.
  /// Compares the saved profile with the server's current profile and sends
  /// changeProfile command if they differ.
  void _ensureProfileIsSet(String? serverCurrentProfile) {
    final config = ref.read(serverConfigProvider);
    final savedProfile = config.profile;
    
    // If server profile is provided and matches saved profile, no action needed
    if (serverCurrentProfile != null && serverCurrentProfile == savedProfile) {
      return;
    }
    
    // If we have available profiles, check if saved profile is valid
    if (state.availableProfiles.isNotEmpty) {
      if (!state.availableProfiles.contains(savedProfile)) {
        // Saved profile is not available, use default or first available
        final profileToUse = state.defaultProfile.isNotEmpty 
            ? state.defaultProfile 
            : state.availableProfiles.first;
        if (profileToUse != savedProfile) {
          ref.read(serverConfigProvider.notifier).updateProfile(profileToUse);
        }
        wsClient.send(ClientCommand.changeProfile(profileToUse));
        return;
      }
    }
    
    // If server profile differs from saved profile, set it
    if (serverCurrentProfile == null || serverCurrentProfile != savedProfile) {
      wsClient.send(ClientCommand.changeProfile(savedProfile));
    }
  }

  void _handleEvent(ServerEvent event) {
    if (event is HealthOkEvent) {
      // Only change pane if we're not already on a specific screen
      // This prevents keepalive health checks from kicking user out of chat
      final shouldChangePane = state.pane == ActivePane.connecting || 
                               state.pane == ActivePane.setup;
      
      // Ensure the saved profile is set when connection is established
      _ensureProfileIsSet(event.profile);
      
      state = state.copyWith(
        connection: ConnectionStatus.online,
        pane: shouldChangePane ? ActivePane.conversations : state.pane,
        error: null,
        currentProfile: event.profile, // Track server's current profile
        connectionAttempt: 0, // Clear connection attempt on success
      );
      // Start connection guard to keep connection alive
      unawaited(guard.startConnectionGuard());
      // Only send listConversations if we're changing to conversations pane
      // Don't auto-load conversation if user is already in a chat
      if (shouldChangePane) {
        wsClient.send(ClientCommand.listConversations(limit: 10));
      }
    } else if (event is ErrorEvent) {
      // Error might indicate stream timeout - reset streaming state
      if (state.streaming) {
        wsClient.setStreaming(false);
      }
      state = state.copyWith(
        connection: ConnectionStatus.error,
        error: event.message,
        pane: ActivePane.setup,
        streaming: false,
      );
    } else if (event is ConversationsListEvent) {
      final sorted = [...event.conversations]
        ..sort((a, b) => b.updatedAt.compareTo(a.updatedAt));
      // If we have existing conversations, check if these are new (pagination) or replacement
      final existingIds = state.conversations.map((c) => c.id).toSet();
      final newIds = sorted.map((c) => c.id).toSet();
      
      // If there's overlap, it's likely a refresh - replace
      // If no overlap and we have existing, it's pagination - append
      final List<ConversationSummary> updatedConversations;
      if (existingIds.intersection(newIds).isNotEmpty || state.conversations.isEmpty) {
        // Refresh or initial load - replace
        updatedConversations = sorted;
      } else {
        // Pagination - append new ones
        updatedConversations = [...state.conversations, ...sorted.where((c) => !existingIds.contains(c.id))];
      }
      
      state = state.copyWith(conversations: updatedConversations);
      
      // Only auto-load conversation if:
      // 1. We don't have an active conversation
      // 2. We're not currently in a chat (to avoid disrupting user)
      // 3. We have conversations available
      if (state.activeConversation == null && 
          state.pane != ActivePane.chat && 
          updatedConversations.isNotEmpty) {
        wsClient.send(ClientCommand.loadConversation(updatedConversations.first.id));
      }
    } else if (event is SearchResultsEvent) {
      state = state.copyWith(searchResults: event.results);
    } else if (event is ConversationLoadedEvent) {
      // Update current profile from conversation or keep existing
      final profileToUse = event.conversation.profileName ?? state.currentProfile;
      state = state.copyWith(
        activeConversation: event.conversation,
        pane: ActivePane.chat,
        chatMessages: _mapMessages(event.conversation.messages),
        streaming: false,
        error: null,
        currentProfile: profileToUse, // Track server's current profile
      );
    } else if (event is ConversationCreatedEvent) {
      wsClient.send(ClientCommand.loadConversation(event.conversationId));
    } else if (event is StreamingStartedEvent) {
      _currentAssistantBubbleId = null; // Reset for new streaming session
      wsClient.setStreaming(true); // Extend timeout and disable health checks during streaming
      state = state.copyWith(streaming: true);
    } else if (event is AssistantDeltaEvent) {
      _applyAssistantDelta(event.chunk);
    } else if (event is ReasoningContentDeltaEvent) {
      _applyReasoningContentDelta(event.chunk);
    } else if (event is AssistantCompleteEvent) {
      _completeAssistant(event.content, event.reasoningContent);
    } else if (event is ToolPlannedEvent) {
      // Tools interrupt assistant stream - reset so next delta creates new bubble
      _currentAssistantBubbleId = null;
      _addToolRequestBubbles(event.tools);
    } else if (event is ToolStartedEvent) {
      _updateToolRequest(event.toolCallId, 'running', event.name, event.paramsJson);
    } else if (event is ToolResultEvent) {
      _addToolResultBubble(event.toolCallId, event.name, event.resultJson);
    } else if (event is ToolErrorEvent) {
      _addToolErrorBubble(event.toolCallId, event.name, event.error);
    } else if (event is ConversationCompleteEvent) {
      _waitingForResponse = false;
      wsClient.setStreaming(false); // Resume normal health checks
      // Don't stop guard here - keep connection guard running
      // The guard.ensureStarted() for streaming will be replaced by connection guard
      // Connection guard continues to keep connection alive
      if (state.backgrounded) {
        unawaited(
          notifications.showResponseNotification(
            title: state.activeConversation?.title ?? 'Luna Chat',
            body: _latestAssistantText(),
          ),
        );
      }
      state = state.copyWith(streaming: false);
      wsClient.send(ClientCommand.listConversations());
    } else if (event is ProfileChangedEvent) {
      ref.read(serverConfigProvider.notifier).updateProfile(event.profile);
      // Ensure the new profile is in the available profiles list
      final profiles = state.availableProfiles.toSet();
      profiles.add(event.profile);
      state = state.copyWith(
        availableProfiles: profiles.toList(),
        currentProfile: event.profile, // Update current server profile
      );
    } else if (event is ProfilesListEvent) {
      final config = ref.read(serverConfigProvider);
      final currentProfile = config.profile;
      final profiles = event.profiles.toSet();
      profiles.add(currentProfile); // Ensure current profile is always included

      state = state.copyWith(
        availableProfiles: profiles.toList(),
        defaultProfile: event.defaultProfile,
      );
      
      // Ensure the saved profile is set after receiving profiles list
      // This handles the case where profiles list arrives before HealthOkEvent
      _ensureProfileIsSet(null);
    } else if (event is ConversationDeletedEvent) {
      // Remove deleted conversation from list
      final updated = state.conversations
          .where((c) => c.id != event.conversationId)
          .toList();
      // If deleted conversation was active, clear it
      final activeConv = state.activeConversation?.id == event.conversationId
          ? null
          : state.activeConversation;
      state = state.copyWith(
        conversations: updated,
        activeConversation: activeConv,
        chatMessages: activeConv == null ? [] : state.chatMessages,
      );
    } else if (event is StreamingStoppedEvent) {
      wsClient.setStreaming(false); // Resume normal health checks
      state = state.copyWith(streaming: false);
    } else if (event is DisconnectedEvent) {
      // Stop connection guard when disconnected
      unawaited(guard.stopConnectionGuard());
      // Connection lost - preserve current screen but show error
      state = state.copyWith(
        connection: ConnectionStatus.error,
        // Don't change pane - keep user on current screen
        error: 'Connection to server lost. Please reconnect.',
      );
    }
  }

  List<ChatMessage> _mapMessages(List<MessageView> messages) {
    final result = <ChatMessage>[];
    
    for (final m in messages) {
      final timestamp = DateTime.fromMillisecondsSinceEpoch(m.timestamp * 1000);
      
      if (m.toolCallId != null) {
        // Tool message - split into request and result bubbles
        // Add tool request bubble (with params)
        result.add(ChatMessage(
          id: '${m.toolCallId}_request',
          role: 'tool',
          content: '🧰 ${m.toolName ?? 'tool'}',
          timestamp: timestamp,
          bubbleType: BubbleType.toolRequest,
          toolCallId: m.toolCallId,
          toolName: m.toolName ?? 'tool',
          toolStatus: m.toolStatus ?? 'done',
          toolParams: m.toolParams,
        ));
        
        // Add tool result bubble (if has result or error)
        if (m.toolResult != null || m.toolStatus == 'error') {
          result.add(ChatMessage(
            id: '${m.toolCallId}_result',
            role: 'tool',
            content: '🧰 ${m.toolName ?? 'tool'}',
            timestamp: timestamp,
            bubbleType: BubbleType.toolResult,
            toolCallId: m.toolCallId,
            toolName: m.toolName ?? 'tool',
            toolStatus: m.toolStatus ?? 'done',
            toolResult: m.toolResult,
            toolError: m.toolStatus == 'error'
                ? (m.toolResult ?? m.toolParams)?.toString()
                : null,
          ));
        }
      } else if (m.isSummary) {
        // Summary message
        result.add(ChatMessage(
          id: m.id,
          role: m.role,
          content: m.content,
          timestamp: timestamp,
          bubbleType: BubbleType.summary,
          isSummary: true,
          summarizedCount: m.summarizedCount,
        ));
      } else {
        // Regular user/assistant message
        result.add(ChatMessage(
          id: m.id,
          role: m.role,
          content: m.content,
          timestamp: timestamp,
          bubbleType: m.role == 'user' ? BubbleType.user : BubbleType.assistant,
          reasoningContent: m.reasoningContent,
        ));
      }
    }
    
    return result;
  }

  void _appendUserMessage(String content) {
    _currentAssistantBubbleId = null; // Reset for new conversation turn
    final entry = ChatMessage(
      id: DateTime.now().microsecondsSinceEpoch.toString(),
      role: 'user',
      content: content,
      timestamp: DateTime.now(),
      bubbleType: BubbleType.user,
    );
    state = state.copyWith(
      chatMessages: [...state.chatMessages, entry],
    );
  }

  void _applyAssistantDelta(String chunk) {
    final messages = [...state.chatMessages];
    
    // Find existing bubble by our tracked ID, or create new one
    if (_currentAssistantBubbleId != null) {
      final idx = messages.indexWhere((m) => m.id == _currentAssistantBubbleId);
      if (idx >= 0) {
        // Append to existing bubble
        final existing = messages[idx];
        messages[idx] = existing.copyWith(
          content: '${existing.content}$chunk',
          isStreaming: true,
        );
        state = state.copyWith(chatMessages: messages);
        return;
      }
    }
    
    // Create new assistant bubble
    final newId = DateTime.now().microsecondsSinceEpoch.toString();
    _currentAssistantBubbleId = newId;
    messages.add(ChatMessage(
      id: newId,
      role: 'assistant',
      content: chunk,
      timestamp: DateTime.now(),
      bubbleType: BubbleType.assistant,
      isStreaming: true,
    ));
    state = state.copyWith(chatMessages: messages);
  }

  void _applyReasoningContentDelta(String chunk) {
    final messages = [...state.chatMessages];
    
    // Find existing bubble by our tracked ID
    if (_currentAssistantBubbleId != null) {
      final idx = messages.indexWhere((m) => m.id == _currentAssistantBubbleId);
      if (idx >= 0) {
        // Append to existing reasoning content
        final existing = messages[idx];
        final currentReasoning = existing.reasoningContent ?? '';
        messages[idx] = existing.copyWith(
          reasoningContent: '$currentReasoning$chunk',
          isStreaming: true,
        );
        state = state.copyWith(chatMessages: messages);
        return;
      }
    }
    
    // If no assistant bubble exists yet, create one with reasoning content
    // This shouldn't normally happen, but handle it gracefully
    final newId = DateTime.now().microsecondsSinceEpoch.toString();
    _currentAssistantBubbleId = newId;
    messages.add(ChatMessage(
      id: newId,
      role: 'assistant',
      content: '',
      timestamp: DateTime.now(),
      bubbleType: BubbleType.assistant,
      isStreaming: true,
      reasoningContent: chunk,
    ));
    state = state.copyWith(chatMessages: messages);
  }

  void _completeAssistant(String content, String? reasoningContent) {
    final messages = [...state.chatMessages];
    
    // Find our tracked bubble or the last assistant bubble
    if (_currentAssistantBubbleId != null) {
      final idx = messages.indexWhere((m) => m.id == _currentAssistantBubbleId);
      if (idx >= 0) {
        messages[idx] = messages[idx].copyWith(
          content: content,
          isStreaming: false,
          reasoningContent: reasoningContent,
        );
        state = state.copyWith(chatMessages: messages);
        return;
      }
    }
    
    // Fallback: update last assistant or create new
    final lastAssistantIdx = messages.lastIndexWhere(
      (m) => m.bubbleType == BubbleType.assistant,
    );
    if (lastAssistantIdx >= 0) {
      messages[lastAssistantIdx] = messages[lastAssistantIdx].copyWith(
        content: content,
        isStreaming: false,
      );
    } else {
      messages.add(ChatMessage(
        id: DateTime.now().microsecondsSinceEpoch.toString(),
        role: 'assistant',
        content: content,
        timestamp: DateTime.now(),
        bubbleType: BubbleType.assistant,
        isStreaming: false,
      ));
    }
    state = state.copyWith(chatMessages: messages);
  }

  /// Add tool request bubbles when tools are planned
  void _addToolRequestBubbles(List<PlannedToolView> plans) {
    final messages = [...state.chatMessages];
    for (final plan in plans) {
      messages.add(ChatMessage(
        id: '${plan.id}_request',
        role: 'tool',
        content: '🧰 ${plan.name}',
        timestamp: DateTime.now(),
        bubbleType: BubbleType.toolRequest,
        toolCallId: plan.id,
        toolName: plan.name,
        toolStatus: 'planned',
        toolParams: plan.paramsJson,
      ));
    }
    state = state.copyWith(chatMessages: messages);
  }

  /// Update tool request bubble when tool starts running
  void _updateToolRequest(
    String toolCallId,
    String status,
    String name,
    dynamic params,
  ) {
    final messages = [...state.chatMessages];
    final idx = messages.lastIndexWhere(
      (m) => m.toolCallId == toolCallId && m.bubbleType == BubbleType.toolRequest,
    );
    
    if (idx >= 0) {
      messages[idx] = messages[idx].copyWith(
        toolStatus: status,
        toolParams: params,
      );
    } else {
      // Tool wasn't planned - create request bubble now
      messages.add(ChatMessage(
        id: '${toolCallId}_request',
        role: 'tool',
        content: '🧰 $name',
        timestamp: DateTime.now(),
        bubbleType: BubbleType.toolRequest,
        toolCallId: toolCallId,
        toolName: name,
        toolStatus: status,
        toolParams: params,
      ));
    }
    state = state.copyWith(chatMessages: messages);
  }

  /// Add separate tool result bubble when tool completes
  void _addToolResultBubble(String toolCallId, String name, dynamic result) {
    final messages = [...state.chatMessages];
    
    // Update request bubble status to done
    final requestIdx = messages.lastIndexWhere(
      (m) => m.toolCallId == toolCallId && m.bubbleType == BubbleType.toolRequest,
    );
    if (requestIdx >= 0) {
      messages[requestIdx] = messages[requestIdx].copyWith(toolStatus: 'done');
    }
    
    // Add result bubble
    messages.add(ChatMessage(
      id: '${toolCallId}_result',
      role: 'tool',
      content: '🧰 $name',
      timestamp: DateTime.now(),
      bubbleType: BubbleType.toolResult,
      toolCallId: toolCallId,
      toolName: name,
      toolStatus: 'done',
      toolResult: result,
    ));
    state = state.copyWith(chatMessages: messages);
  }

  /// Add tool error bubble
  void _addToolErrorBubble(String toolCallId, String name, String error) {
    final messages = [...state.chatMessages];
    
    // Update request bubble status to error
    final requestIdx = messages.lastIndexWhere(
      (m) => m.toolCallId == toolCallId && m.bubbleType == BubbleType.toolRequest,
    );
    if (requestIdx >= 0) {
      messages[requestIdx] = messages[requestIdx].copyWith(toolStatus: 'error');
    }
    
    // Add error result bubble
    messages.add(ChatMessage(
      id: '${toolCallId}_error',
      role: 'tool',
      content: '🧰 $name',
      timestamp: DateTime.now(),
      bubbleType: BubbleType.toolResult,
      toolCallId: toolCallId,
      toolName: name,
      toolStatus: 'error',
      toolError: error,
    ));
    state = state.copyWith(chatMessages: messages);
  }

  String _latestAssistantText() {
    final lastAssistant = state.chatMessages.lastWhere(
      (m) => m.bubbleType == BubbleType.assistant,
      orElse: () => ChatMessage(
        id: 'noop',
        role: 'assistant',
        content: 'Ready to help',
        timestamp: DateTime.fromMillisecondsSinceEpoch(0),
        bubbleType: BubbleType.assistant,
      ),
    );
    return lastAssistant.content;
  }
}
