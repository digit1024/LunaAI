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

final wsClientProvider = Provider<LunaWsClient>((ref) {
  final config = ref.watch(serverConfigProvider);
  return LunaWsClient(config);
});

final appControllerProvider =
    StateNotifierProvider<AppController, AppState>((ref) {
  final wsClient = ref.watch(wsClientProvider);
  final notifications = ref.watch(notificationServiceProvider);
  final guard = ref.watch(foregroundGuardProvider);
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

  Future<void> init() async {
    if (_initialized) return;
    _initialized = true;
    await notifications.init();
    await guard.init();
  }

  Future<void> connect() async {
    state = state.copyWith(pane: ActivePane.connecting, error: null);
    await wsClient.connect();
    _subscription = wsClient.events.listen(_handleEvent);
    wsClient.send(ClientCommand.healthCheck());
    wsClient.send(ClientCommand.listConversations());
    wsClient.send(ClientCommand.listProfiles());
  }

  @override
  void dispose() {
    unawaited(_subscription?.cancel());
    unawaited(wsClient.dispose());
    super.dispose();
  }

  void refreshConversations() {
    wsClient.send(ClientCommand.listConversations());
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

  void selectConversation(String id) {
    wsClient.send(ClientCommand.loadConversation(id));
    state = state.copyWith(pane: ActivePane.chat);
  }

  void setBackgrounded(bool value) {
    state = state.copyWith(backgrounded: value);
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
      state = state.copyWith(
        connection: ConnectionStatus.online,
        pane: ActivePane.conversations,
        error: null,
      );
      wsClient.send(ClientCommand.listConversations());
    } else if (event is ErrorEvent) {
      state = state.copyWith(
        connection: ConnectionStatus.error,
        error: event.message,
        pane: ActivePane.setup,
      );
    } else if (event is ConversationsListEvent) {
      final sorted = [...event.conversations]
        ..sort((a, b) => b.updatedAt.compareTo(a.updatedAt));
      state = state.copyWith(conversations: sorted);
      if (state.activeConversation == null && sorted.isNotEmpty) {
        wsClient.send(ClientCommand.loadConversation(sorted.first.id));
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
      guard.stop();
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
    }
  }

  List<ChatMessage> _mapMessages(List<MessageView> messages) {
    return messages
        .map(
          (m) => ChatMessage(
            id: m.id,
            role: m.role,
            content: m.content,
            timestamp:
                DateTime.fromMillisecondsSinceEpoch(m.timestamp * 1000),
            toolChip: m.toolCallId != null
                ? ToolCallChip(
                    id: m.toolCallId!,
                    name: m.toolName ?? 'tool',
                    status: m.toolStatus ?? 'pending',
                    description:
                        (m.toolResult ?? m.toolParams)?.toString() ?? '',
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
          description: plan.paramsJson?.toString() ?? '',
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
    final idx =
        messages.lastIndexWhere((m) => m.toolChip?.id == toolCallId);
    if (idx >= 0) {
      final message = messages[idx];
      messages[idx] = message.copyWith(
        toolChip: message.toolChip?.copyWith(
          status: status,
          description: payload?.toString() ?? '',
        ),
        content: '🧰 $name',
      );
    } else {
      messages.add(ChatMessage(
        id: toolCallId,
        role: 'tool',
        content: '🧰 $name',
        timestamp: DateTime.now(),
        toolChip: ToolCallChip(
          id: toolCallId,
          name: name,
          status: status,
          description: payload?.toString() ?? '',
        ),
      ));
    }
    state = state.copyWith(chatMessages: messages);
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
