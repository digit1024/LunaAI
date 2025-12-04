import 'package:equatable/equatable.dart';

import '../data/http/file_client.dart';
import '../data/ws/ws_dto.dart';

enum ConnectionStatus { connecting, online, error }

enum ActivePane { setup, connecting, conversations, chat, settings }

enum DialogModeState { listening, processing, speaking }

/// Bubble types for simplified display logic
enum BubbleType {
  user,        // User message
  assistant,   // Assistant text content
  toolRequest, // Tool call with parameters (when tool starts)
  toolResult,  // Tool result (when tool completes)
}

class ChatMessage extends Equatable {
  final String id;
  final String role;
  final String content;
  final bool isStreaming;
  final DateTime timestamp;
  final BubbleType bubbleType;
  final String? toolCallId;   // For tool bubbles
  final String? toolName;     // For tool bubbles
  final String? toolStatus;   // planned, running, done, error
  final dynamic toolParams;   // For toolRequest bubbles
  final dynamic toolResult;   // For toolResult bubbles
  final String? toolError;    // For error state
  final String? reasoningContent; // For DeepSeek thinking/reasoning content

  const ChatMessage({
    required this.id,
    required this.role,
    required this.content,
    required this.timestamp,
    required this.bubbleType,
    this.isStreaming = false,
    this.toolCallId,
    this.toolName,
    this.toolStatus,
    this.toolParams,
    this.toolResult,
    this.toolError,
    this.reasoningContent,
  });

  ChatMessage copyWith({
    String? id,
    String? role,
    String? content,
    bool? isStreaming,
    DateTime? timestamp,
    BubbleType? bubbleType,
    String? toolCallId,
    String? toolName,
    String? toolStatus,
    Object? toolParams = _unset,
    Object? toolResult = _unset,
    Object? toolError = _unset,
    String? reasoningContent,
  }) {
    return ChatMessage(
      id: id ?? this.id,
      role: role ?? this.role,
      content: content ?? this.content,
      timestamp: timestamp ?? this.timestamp,
      bubbleType: bubbleType ?? this.bubbleType,
      isStreaming: isStreaming ?? this.isStreaming,
      toolCallId: toolCallId ?? this.toolCallId,
      toolName: toolName ?? this.toolName,
      toolStatus: toolStatus ?? this.toolStatus,
      toolParams: identical(toolParams, _unset) ? this.toolParams : toolParams,
      toolResult: identical(toolResult, _unset) ? this.toolResult : toolResult,
      toolError: identical(toolError, _unset) ? this.toolError : toolError as String?,
      reasoningContent: reasoningContent ?? this.reasoningContent,
    );
  }

  static const Object _unset = Object();

  @override
  List<Object?> get props => [
        id,
        role,
        content,
        isStreaming,
        timestamp,
        bubbleType,
        toolCallId,
        toolName,
        toolStatus,
        toolParams,
        toolResult,
        toolError,
        reasoningContent,
      ];
}

/// Legacy ToolCallChip - kept for backwards compatibility with loaded messages
/// New code should use ChatMessage with BubbleType.toolRequest/toolResult
class ToolCallChip extends Equatable {
  static const Object _unset = Object();

  final String id;
  final String name;
  final String status;
  final dynamic params;
  final dynamic result;
  final String? error;

  const ToolCallChip({
    required this.id,
    required this.name,
    required this.status,
    this.params,
    this.result,
    this.error,
  });

  ToolCallChip copyWith({
    String? status,
    Object? params = _unset,
    Object? result = _unset,
    Object? error = _unset,
  }) {
    return ToolCallChip(
      id: id,
      name: name,
      status: status ?? this.status,
      params: identical(params, _unset) ? this.params : params,
      result: identical(result, _unset) ? this.result : result,
      error: identical(error, _unset) ? this.error : error as String?,
    );
  }

  @override
  List<Object?> get props => [id, name, status, params, result, error];
}

class AppState extends Equatable {
  final ConnectionStatus connection;
  final ActivePane pane;
  final List<ConversationSummary> conversations;
  final List<SearchResult> searchResults;
  final String searchQuery;
  final ConversationView? activeConversation;
  final List<ChatMessage> chatMessages;
  final bool streaming;
  final bool backgrounded;
  final String? error;
  final List<String> availableProfiles;
  final String defaultProfile;
  final String currentProfile; // Current server profile (from server, not config)
  final int
      connectionAttempt; // Current attempt (1, 2, 3) or 0 if not connecting
  final bool isDialogModeActive;
  final DialogModeState dialogModeState;
  final List<FileAttachment> attachedFiles;

  const AppState({
    required this.connection,
    required this.pane,
    required this.conversations,
    required this.searchResults,
    required this.searchQuery,
    required this.activeConversation,
    required this.chatMessages,
    required this.streaming,
    required this.backgrounded,
    required this.error,
    required this.availableProfiles,
    required this.defaultProfile,
    required this.currentProfile,
    required this.connectionAttempt,
    required this.isDialogModeActive,
    required this.dialogModeState,
    required this.attachedFiles,
  });

  factory AppState.initial() => const AppState(
        connection: ConnectionStatus.connecting,
        pane: ActivePane.connecting, // Start with connecting screen
        conversations: [],
        searchResults: [],
        searchQuery: '',
        activeConversation: null,
        chatMessages: [],
        streaming: false,
        backgrounded: false,
        error: null,
        availableProfiles: [],
        defaultProfile: '',
        currentProfile: '',
        connectionAttempt: 0,
        isDialogModeActive: false,
        dialogModeState: DialogModeState.listening,
        attachedFiles: [],
      );

  AppState copyWith({
    ConnectionStatus? connection,
    ActivePane? pane,
    List<ConversationSummary>? conversations,
    List<SearchResult>? searchResults,
    String? searchQuery,
    ConversationView? activeConversation,
    List<ChatMessage>? chatMessages,
    bool? streaming,
    bool? backgrounded,
    String? error,
    List<String>? availableProfiles,
    String? defaultProfile,
    String? currentProfile,
    int? connectionAttempt,
    bool? isDialogModeActive,
    DialogModeState? dialogModeState,
    List<FileAttachment>? attachedFiles,
  }) {
    return AppState(
      connection: connection ?? this.connection,
      pane: pane ?? this.pane,
      conversations: conversations ?? this.conversations,
      searchResults: searchResults ?? this.searchResults,
      searchQuery: searchQuery ?? this.searchQuery,
      activeConversation: activeConversation ?? this.activeConversation,
      chatMessages: chatMessages ?? this.chatMessages,
      streaming: streaming ?? this.streaming,
      backgrounded: backgrounded ?? this.backgrounded,
      error: error,
      availableProfiles: availableProfiles ?? this.availableProfiles,
      defaultProfile: defaultProfile ?? this.defaultProfile,
      currentProfile: currentProfile ?? this.currentProfile,
      connectionAttempt: connectionAttempt ?? this.connectionAttempt,
      isDialogModeActive: isDialogModeActive ?? this.isDialogModeActive,
      dialogModeState: dialogModeState ?? this.dialogModeState,
      attachedFiles: attachedFiles ?? this.attachedFiles,
    );
  }

  @override
  List<Object?> get props => [
        connection,
        pane,
        conversations,
        searchResults,
        searchQuery,
        activeConversation,
        chatMessages,
        streaming,
        backgrounded,
        error,
        availableProfiles,
        defaultProfile,
        currentProfile,
        connectionAttempt,
        isDialogModeActive,
        dialogModeState,
        attachedFiles,
      ];
}
