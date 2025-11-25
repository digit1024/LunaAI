import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../core/config/server_config.dart';
import '../data/ws/luna_ws_client.dart';
import '../data/ws/ws_dto.dart';
import '../services/foreground_guard.dart';
import '../services/notification_service.dart';
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
    StateNotifierProvider<AppController, AppState>((ref) {
  // Use ref.read to avoid recreation on changes
  final wsClient = ref.read(wsClientProvider);
  final notifications = ref.read(notificationServiceProvider);
  final guard = ref.read(foregroundGuardProvider);
  final controller = AppController(
    ref,
    wsClient: wsClient,
    notifications: notifications,
    guard: guard,
  );
  controller.init();
  ref.onDispose(controller.dispose);
  return controller;
});

class AppController extends StateNotifier<AppState> {
  AppController(
    this.ref, {
    required this.wsClient,
    required this.notifications,
    required this.guard,
  }) : super(AppState.initial());

  final Ref ref;
  final LunaWsClient wsClient;
  final NotificationService notifications;
  final ForegroundGuard guard;

  StreamSubscription<ServerEvent>? _subscription;
  bool _initialized = false;
  bool _waitingForResponse = false;
  bool _connecting = false;

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
  Future<void> connect() async {
    if (_connecting) return; // Prevent concurrent connect calls
    _connecting = true;

    // Cancel any existing subscription to prevent duplicates
    await _subscription?.cancel();
    _subscription = null;

    final config = ref.read(serverConfigProvider);

    for (int attempt = 1; attempt <= _maxConnectionAttempts; attempt++) {
      state = state.copyWith(
        pane: ActivePane.connecting,
        error: null,
        connectionAttempt: attempt,
      );

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

    // All attempts failed - fall back to setup screen
    _connecting = false;
    state = state.copyWith(
      pane: ActivePane.setup,
      connection: ConnectionStatus.error,
      error: 'Could not connect after $_maxConnectionAttempts attempts.\n'
          'Please check server settings and try again.',
      connectionAttempt: 0,
    );
  }

  @override
  void dispose() {
    unawaited(_subscription?.cancel());
    unawaited(guard.stopConnectionGuard());
    unawaited(wsClient.dispose());
    super.dispose();
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
  Future<void> checkAndReconnect() async {
    if (wsClient.isConnected) {
      // Verify connection is actually alive with a health check
      wsClient.send(ClientCommand.healthCheck());
      // If connection is good, ensure guard is running
      if (state.connection == ConnectionStatus.online) {
        unawaited(guard.startConnectionGuard());
      }
    } else {
      // Connection is dead, attempt to reconnect
      debugPrint('Connection lost, attempting to reconnect...');
      await connect();
    }
  }

  Future<void> startNewConversation() async {
    wsClient.send(ClientCommand.startConversation('Generating title...'));
  }

  Future<void> changeProfile(String profile) async {
    wsClient.send(ClientCommand.changeProfile(profile));
  }

  Future<void> sendPrompt(String text) async {
    if (text.trim().isEmpty) return;
    final conversationId = state.activeConversation?.id;
    _appendUserMessage(text.trim());
    await guard.ensureStarted('Streaming reply');
    _waitingForResponse = true;
    wsClient.send(ClientCommand.sendMessage(
      conversationId: conversationId,
      content: text.trim(),
    ));
  }

  void _handleEvent(ServerEvent event) {
    if (event is HealthOkEvent) {
      // Only change pane if we're not already on a specific screen
      // This prevents keepalive health checks from kicking user out of chat
      final shouldChangePane = state.pane == ActivePane.connecting || 
                               state.pane == ActivePane.setup;
      
      state = state.copyWith(
        connection: ConnectionStatus.online,
        pane: shouldChangePane ? ActivePane.conversations : state.pane,
        error: null,
      );
      // Start connection guard to keep connection alive
      unawaited(guard.startConnectionGuard());
      // Only send listConversations if we're changing to conversations pane
      if (shouldChangePane) {
        wsClient.send(ClientCommand.listConversations(limit: 10));
      }
    } else if (event is ErrorEvent) {
      state = state.copyWith(
        connection: ConnectionStatus.error,
        error: event.message,
        pane: ActivePane.setup,
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
      if (state.activeConversation == null && updatedConversations.isNotEmpty) {
        wsClient.send(ClientCommand.loadConversation(updatedConversations.first.id));
      }
    } else if (event is SearchResultsEvent) {
      state = state.copyWith(searchResults: event.results);
    } else if (event is ConversationLoadedEvent) {
      state = state.copyWith(
        activeConversation: event.conversation,
        pane: ActivePane.chat,
        chatMessages: _mapMessages(event.conversation.messages),
        streaming: false,
        error: null,
      );
    } else if (event is ConversationCreatedEvent) {
      wsClient.send(ClientCommand.loadConversation(event.conversationId));
    } else if (event is StreamingStartedEvent) {
      state = state.copyWith(streaming: true);
    } else if (event is AssistantDeltaEvent) {
      _applyAssistantDelta(event.chunk);
    } else if (event is AssistantCompleteEvent) {
      _completeAssistant(event.content);
    } else if (event is ToolPlannedEvent) {
      _injectToolPlans(event.tools);
    } else if (event is ToolStartedEvent) {
      _markTool(event.toolCallId, 'running', event.name, event.paramsJson);
    } else if (event is ToolResultEvent) {
      _markTool(event.toolCallId, 'done', event.name, event.resultJson);
    } else if (event is ToolErrorEvent) {
      _markTool(event.toolCallId, 'error', event.name, event.error);
    } else if (event is ConversationCompleteEvent) {
      _waitingForResponse = false;
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
      state = state.copyWith(availableProfiles: profiles.toList());
    } else if (event is ProfilesListEvent) {
      final config = ref.read(serverConfigProvider);
      final currentProfile = config.profile;
      final profiles = event.profiles.toSet();
      profiles.add(currentProfile); // Ensure current profile is always included

      state = state.copyWith(
        availableProfiles: profiles.toList(),
        defaultProfile: event.defaultProfile,
      );
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
    return messages
        .map(
          (m) => ChatMessage(
            id: m.id,
            role: m.role,
            content: m.content,
            timestamp: DateTime.fromMillisecondsSinceEpoch(m.timestamp * 1000),
            toolChip: m.toolCallId != null
                ? ToolCallChip(
                    id: m.toolCallId!,
                    name: m.toolName ?? 'tool',
                    status: m.toolStatus ?? 'pending',
                    params: m.toolParams,
                    result: m.toolResult,
                    error: m.toolStatus == 'error'
                        ? (m.toolResult ?? m.toolParams)?.toString()
                        : null,
                  )
                : null,
          ),
        )
        .toList();
  }

  void _appendUserMessage(String content) {
    final entry = ChatMessage(
      id: DateTime.now().microsecondsSinceEpoch.toString(),
      role: 'user',
      content: content,
      timestamp: DateTime.now(),
    );
    state = state.copyWith(
      chatMessages: [...state.chatMessages, entry],
    );
  }

  void _applyAssistantDelta(String chunk) {
    final messages = [...state.chatMessages];
    if (messages.isEmpty || messages.last.role != 'assistant') {
      messages.add(ChatMessage(
        id: DateTime.now().microsecondsSinceEpoch.toString(),
        role: 'assistant',
        content: chunk,
        timestamp: DateTime.now(),
        isStreaming: true,
      ));
    } else {
      final last = messages.last;
      messages[messages.length - 1] = last.copyWith(
        content: '${last.content}$chunk',
        isStreaming: true,
      );
    }
    state = state.copyWith(chatMessages: messages);
  }

  void _completeAssistant(String content) {
    final messages = [...state.chatMessages];
    if (messages.isEmpty || messages.last.role != 'assistant') {
      messages.add(ChatMessage(
        id: DateTime.now().microsecondsSinceEpoch.toString(),
        role: 'assistant',
        content: content,
        timestamp: DateTime.now(),
        isStreaming: false,
      ));
    } else {
      final last = messages.last;
      messages[messages.length - 1] =
          last.copyWith(content: content, isStreaming: false);
    }
    state = state.copyWith(chatMessages: messages);
  }

  void _injectToolPlans(List<PlannedToolView> plans) {
    final messages = [...state.chatMessages];
    for (final plan in plans) {
      messages.add(ChatMessage(
        id: plan.id,
        role: 'tool',
        content: '🧰 ${plan.name}',
        timestamp: DateTime.now(),
        toolChip: ToolCallChip(
          id: plan.id,
          name: plan.name,
          status: 'planned',
          params: plan.paramsJson,
        ),
      ));
    }
    state = state.copyWith(chatMessages: messages);
  }

  void _markTool(
    String toolCallId,
    String status,
    String name,
    dynamic payload,
  ) {
    final messages = [...state.chatMessages];
    final idx = messages.lastIndexWhere((m) => m.toolChip?.id == toolCallId);
    if (idx >= 0) {
      final message = messages[idx];
      messages[idx] = message.copyWith(
        toolChip: _updatedToolChip(
          original: message.toolChip,
          status: status,
          payload: payload,
          toolId: toolCallId,
          toolName: name,
        ),
        content: '🧰 $name',
      );
    } else {
      messages.add(ChatMessage(
        id: toolCallId,
        role: 'tool',
        content: '🧰 $name',
        timestamp: DateTime.now(),
        toolChip: _buildToolChip(
          id: toolCallId,
          name: name,
          status: status,
          payload: payload,
        ),
      ));
    }
    state = state.copyWith(chatMessages: messages);
  }

  ToolCallChip _updatedToolChip({
    required ToolCallChip? original,
    required String status,
    required String toolId,
    required String toolName,
    dynamic payload,
  }) {
    final chip =
        original ?? ToolCallChip(id: toolId, name: toolName, status: status);
    switch (status) {
      case 'planned':
      case 'running':
        return chip.copyWith(status: status, params: payload);
      case 'done':
        return chip.copyWith(status: status, result: payload);
      case 'error':
        return chip.copyWith(status: status, error: payload?.toString());
      default:
        return chip.copyWith(status: status);
    }
  }

  ToolCallChip _buildToolChip({
    required String id,
    required String name,
    required String status,
    dynamic payload,
  }) {
    switch (status) {
      case 'planned':
      case 'running':
        return ToolCallChip(
          id: id,
          name: name,
          status: status,
          params: payload,
        );
      case 'done':
        return ToolCallChip(
          id: id,
          name: name,
          status: status,
          result: payload,
        );
      case 'error':
        return ToolCallChip(
          id: id,
          name: name,
          status: status,
          error: payload?.toString(),
        );
      default:
        return ToolCallChip(
          id: id,
          name: name,
          status: status,
        );
    }
  }

  String _latestAssistantText() {
    final lastAssistant = state.chatMessages.lastWhere(
      (m) => m.role == 'assistant',
      orElse: () => ChatMessage(
        id: 'noop',
        role: 'assistant',
        content: 'Ready to help',
        timestamp: DateTime.fromMillisecondsSinceEpoch(0),
      ),
    );
    return lastAssistant.content;
  }
}
