enum MCPServerStatusEnum { connected, failed }

class MCPServerInfo {
  final String name;
  final int toolCount;
  final List<String> tools;
  final MCPServerStatusEnum status;
  final String? error;

  const MCPServerInfo({
    required this.name,
    required this.toolCount,
    required this.tools,
    required this.status,
    this.error,
  });

  factory MCPServerInfo.fromJson(Map<String, dynamic> json) {
    final statusStr = json['status'] as String?;
    return MCPServerInfo(
      name: json['name'] as String? ?? '',
      toolCount: (json['tool_count'] as num?)?.toInt() ?? 0,
      tools: (json['tools'] as List<dynamic>?)
              ?.map((entry) => entry.toString())
              .toList() ??
          const [],
      status: statusStr == 'failed'
          ? MCPServerStatusEnum.failed
          : MCPServerStatusEnum.connected,
      error: json['error'] as String?,
    );
  }
}

class MCPServersResponse {
  final List<MCPServerInfo> servers;

  const MCPServersResponse({required this.servers});

  factory MCPServersResponse.fromJson(Map<String, dynamic> json) {
    final rawServers = json['servers'] as List<dynamic>? ?? const [];
    return MCPServersResponse(
      servers: rawServers
          .map((entry) => MCPServerInfo.fromJson(entry as Map<String, dynamic>))
          .toList(),
    );
  }
}
