import 'dart:convert';

sealed class ServerEvent {
  const ServerEvent();

  factory ServerEvent.fromJson(Map<String, dynamic> json) {
    switch (json['type']) {
      case 'health_ok':
        return HealthOkEvent(
          timestamp: json['timestamp'] as int? ?? 0,
          profile: json['profile'] as String? ?? '',
        );
      case 'error':
        return ErrorEvent(json['message'] as String? ?? 'Unknown error');
      case 'conversation_created':
        return ConversationCreatedEvent(json['conversation_id'] as String);
      case 'conversation_loaded':
        return ConversationLoadedEvent(
          ConversationView.fromJson(json['conversation']),
        );
      case 'conversations_list':
        return ConversationsListEvent(
          (json['conversations'] as List<dynamic>)
              .map((e) => ConversationSummary.fromJson(e))
              .toList(),
        );
      case 'search_results':
        return SearchResultsEvent(
          (json['results'] as List<dynamic>)
              .map((e) => SearchResult.fromJson(e))
              .toList(),
        );
      case 'profile_changed':
        return ProfileChangedEvent(json['profile'] as String);
      case 'profiles_list':
        return ProfilesListEvent(
          (json['profiles'] as List<dynamic>).map((e) => e as String).toList(),
          json['default_profile'] as String,
        );
      case 'message_accepted':
        return MessageAcceptedEvent(json['conversation_id'] as String);
      case 'streaming_started':
        return StreamingStartedEvent(json['conversation_id'] as String);
      case 'assistant_delta':
        return AssistantDeltaEvent(
          json['conversation_id'] as String,
          json['chunk'] as String? ?? '',
          json['seq'] as int? ?? 0,
        );
      case 'assistant_complete':
        return AssistantCompleteEvent(
          json['conversation_id'] as String,
          json['content'] as String? ?? '',
        );
      case 'tool_planned':
        return ToolPlannedEvent(
          json['conversation_id'] as String,
          (json['tools'] as List<dynamic>)
              .map((e) => PlannedToolView.fromJson(e))
              .toList(),
        );
      case 'tool_started':
        return ToolStartedEvent(
          json['conversation_id'] as String,
          json['tool_call_id'] as String,
          json['name'] as String? ?? 'tool',
          json['params_json'],
        );
      case 'tool_result':
        return ToolResultEvent(
          json['conversation_id'] as String,
          json['tool_call_id'] as String,
          json['name'] as String? ?? 'tool',
          json['result_json'],
        );
      case 'tool_error':
        return ToolErrorEvent(
          json['conversation_id'] as String,
          json['tool_call_id'] as String,
          json['name'] as String? ?? 'tool',
          json['error'] as String? ?? 'Tool error',
        );
      case 'conversation_complete':
        return ConversationCompleteEvent(json['conversation_id'] as String);
      case 'conversation_deleted':
        return ConversationDeletedEvent(json['conversation_id'] as String);
      case 'streaming_stopped':
        return StreamingStoppedEvent(json['conversation_id'] as String);
      default:
        return UnknownEvent(jsonEncode(json));
    }
  }
}

class HealthOkEvent extends ServerEvent {
  final int timestamp;
  final String profile;

  const HealthOkEvent({required this.timestamp, required this.profile});
}

class ErrorEvent extends ServerEvent {
  final String message;
  const ErrorEvent(this.message);
}

class ConversationCreatedEvent extends ServerEvent {
  final String conversationId;
  const ConversationCreatedEvent(this.conversationId);
}

class ConversationLoadedEvent extends ServerEvent {
  final ConversationView conversation;
  const ConversationLoadedEvent(this.conversation);
}

class ConversationsListEvent extends ServerEvent {
  final List<ConversationSummary> conversations;
  const ConversationsListEvent(this.conversations);
}

class SearchResultsEvent extends ServerEvent {
  final List<SearchResult> results;
  const SearchResultsEvent(this.results);
}

class ProfileChangedEvent extends ServerEvent {
  final String profile;
  const ProfileChangedEvent(this.profile);
}

class ProfilesListEvent extends ServerEvent {
  final List<String> profiles;
  final String defaultProfile;
  const ProfilesListEvent(this.profiles, this.defaultProfile);
}

class MessageAcceptedEvent extends ServerEvent {
  final String conversationId;
  const MessageAcceptedEvent(this.conversationId);
}

class StreamingStartedEvent extends ServerEvent {
  final String conversationId;
  const StreamingStartedEvent(this.conversationId);
}

class AssistantDeltaEvent extends ServerEvent {
  final String conversationId;
  final String chunk;
  final int seq;

  const AssistantDeltaEvent(this.conversationId, this.chunk, this.seq);
}

class AssistantCompleteEvent extends ServerEvent {
  final String conversationId;
  final String content;

  const AssistantCompleteEvent(this.conversationId, this.content);
}

class ToolPlannedEvent extends ServerEvent {
  final String conversationId;
  final List<PlannedToolView> tools;

  const ToolPlannedEvent(this.conversationId, this.tools);
}

class ToolStartedEvent extends ServerEvent {
  final String conversationId;
  final String toolCallId;
  final String name;
  final dynamic paramsJson;

  const ToolStartedEvent(
    this.conversationId,
    this.toolCallId,
    this.name,
    this.paramsJson,
  );
}

class ToolResultEvent extends ServerEvent {
  final String conversationId;
  final String toolCallId;
  final String name;
  final dynamic resultJson;

  const ToolResultEvent(
    this.conversationId,
    this.toolCallId,
    this.name,
    this.resultJson,
  );
}

class ToolErrorEvent extends ServerEvent {
  final String conversationId;
  final String toolCallId;
  final String name;
  final String error;

  const ToolErrorEvent(
    this.conversationId,
    this.toolCallId,
    this.name,
    this.error,
  );
}

class ConversationCompleteEvent extends ServerEvent {
  final String conversationId;
  const ConversationCompleteEvent(this.conversationId);
}

class UnknownEvent extends ServerEvent {
  final String payload;
  const UnknownEvent(this.payload);
}

class ConversationDeletedEvent extends ServerEvent {
  final String conversationId;
  const ConversationDeletedEvent(this.conversationId);
}

class StreamingStoppedEvent extends ServerEvent {
  final String conversationId;
  const StreamingStoppedEvent(this.conversationId);
}

/// Emitted when the WebSocket connection is lost.
/// UI should handle this by showing reconnect option or trying to reconnect.
class DisconnectedEvent extends ServerEvent {
  const DisconnectedEvent();
}

class ConversationSummary {
  final String id;
  final String title;
  final String? lastMessagePreview;
  final int updatedAt;

  ConversationSummary({
    required this.id,
    required this.title,
    required this.lastMessagePreview,
    required this.updatedAt,
  });

  factory ConversationSummary.fromJson(Map<String, dynamic> json) {
    return ConversationSummary(
      id: json['id'] as String,
      title: json['title'] as String? ?? 'Conversation',
      lastMessagePreview: json['last_message_preview'] as String?,
      updatedAt: json['updated_at'] as int? ?? 0,
    );
  }
}

class SearchResult {
  final String conversationId;
  final String snippet;
  final int timestamp;
  final double rank;

  SearchResult({
    required this.conversationId,
    required this.snippet,
    required this.timestamp,
    required this.rank,
  });

  factory SearchResult.fromJson(Map<String, dynamic> json) {
    return SearchResult(
      conversationId: json['conversation_id'] as String,
      snippet: json['snippet'] as String? ?? '',
      timestamp: json['timestamp'] as int? ?? 0,
      rank: (json['rank'] as num?)?.toDouble() ?? 0,
    );
  }
}

class ConversationView {
  final String id;
  final String title;
  final int createdAt;
  final int updatedAt;
  final List<MessageView> messages;

  ConversationView({
    required this.id,
    required this.title,
    required this.createdAt,
    required this.updatedAt,
    required this.messages,
  });

  factory ConversationView.fromJson(Map<String, dynamic> json) {
    return ConversationView(
      id: json['id'] as String,
      title: json['title'] as String? ?? 'Conversation',
      createdAt: json['created_at'] as int? ?? 0,
      updatedAt: json['updated_at'] as int? ?? 0,
      messages: (json['messages'] as List<dynamic>? ?? [])
          .map((entry) => MessageView.fromJson(entry))
          .toList(),
    );
  }
}

class MessageView {
  final String id;
  final String role;
  final String content;
  final int timestamp;
  final String? toolCallId;
  final String? toolName;
  final String? toolStatus;
  final dynamic toolParams;
  final dynamic toolResult;

  MessageView({
    required this.id,
    required this.role,
    required this.content,
    required this.timestamp,
    this.toolCallId,
    this.toolName,
    this.toolStatus,
    this.toolParams,
    this.toolResult,
  });

  factory MessageView.fromJson(Map<String, dynamic> json) {
    return MessageView(
      id: json['id'] as String,
      role: json['role'] as String? ?? 'assistant',
      content: json['content'] as String? ?? '',
      timestamp: json['timestamp'] as int? ?? 0,
      toolCallId: json['tool_call_id'] as String?,
      toolName: json['tool_name'] as String?,
      toolStatus: json['tool_status'] as String?,
      toolParams: json['tool_params_json'],
      toolResult: json['tool_result_json'],
    );
  }
}

class PlannedToolView {
  final String id;
  final String name;
  final dynamic paramsJson;

  PlannedToolView({
    required this.id,
    required this.name,
    required this.paramsJson,
  });

  factory PlannedToolView.fromJson(Map<String, dynamic> json) {
    return PlannedToolView(
      id: json['id'] as String,
      name: json['name'] as String? ?? 'tool',
      paramsJson: json['params_json'],
    );
  }
}

class ClientCommand {
  final String type;
  final Map<String, dynamic> payload;

  ClientCommand(this.type, [Map<String, dynamic>? payload])
      : payload = payload ?? {};

  Map<String, dynamic> toJson() => {'type': type, ...payload};

  static ClientCommand healthCheck() => ClientCommand('health_check');
  static ClientCommand listConversations({int? offset, int? limit}) =>
      ClientCommand('list_conversations', {
        if (offset != null) 'offset': offset,
        if (limit != null) 'limit': limit,
      });
  static ClientCommand search(String query) =>
      ClientCommand('list_conversations', {'query': query});
  static ClientCommand loadConversation(String id) =>
      ClientCommand('load_conversation', {'conversation_id': id});
  static ClientCommand startConversation(String title) =>
      ClientCommand('start_conversation', {'title': title});
  static ClientCommand sendMessage({
    String? conversationId,
    required String content,
  }) =>
      ClientCommand('send_message', {
        if (conversationId != null) 'conversation_id': conversationId,
        'content': content,
      });
  static ClientCommand changeProfile(String profile) =>
      ClientCommand('change_profile', {'profile': profile});
  static ClientCommand listProfiles() => ClientCommand('list_profiles');
  static ClientCommand deleteConversation(String conversationId) =>
      ClientCommand('delete_conversation', {'conversation_id': conversationId});
  static ClientCommand stopStreaming({String? conversationId}) =>
      ClientCommand('stop_streaming', {
        if (conversationId != null) 'conversation_id': conversationId,
      });
}


