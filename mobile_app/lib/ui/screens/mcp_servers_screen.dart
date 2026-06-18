import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../application/app_controller.dart';
import '../../application/app_state.dart';
import '../../data/http/mcp_dto.dart';

class McpServersScreen extends ConsumerStatefulWidget {
  const McpServersScreen({super.key});

  @override
  ConsumerState<McpServersScreen> createState() => _McpServersScreenState();
}

class _McpServersScreenState extends ConsumerState<McpServersScreen> {
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      ref.read(appControllerProvider.notifier).loadMcpServers();
    });
  }

  @override
  Widget build(BuildContext context) {
    final state = ref.watch(appControllerProvider);
    final controller = ref.read(appControllerProvider.notifier);

    ref.listen<String?>(
      appControllerProvider.select((s) => s.error),
      (_, error) {
        if (error != null &&
            error.isNotEmpty &&
            error.contains('MCP servers')) {
          ScaffoldMessenger.of(context).showSnackBar(
            SnackBar(content: Text(error)),
          );
        }
      },
    );

    return Scaffold(
      appBar: AppBar(
        title: const Text('MCP Servers'),
        leading: IconButton(
          icon: const Icon(Icons.arrow_back),
          onPressed: controller.openConversations,
        ),
        actions: [
          IconButton(
            tooltip: 'Refresh',
            icon: const Icon(Icons.refresh),
            onPressed: () => controller.loadMcpServers(),
          ),
        ],
      ),
      body: _McpServersBody(state: state, controller: controller),
    );
  }
}

class _McpServersBody extends StatelessWidget {
  const _McpServersBody({
    required this.state,
    required this.controller,
  });

  final AppState state;
  final AppController controller;

  @override
  Widget build(BuildContext context) {
    if (state.connection != ConnectionStatus.online) {
      return const Center(
        child: Padding(
          padding: EdgeInsets.all(24),
          child: Text('Connect to a server to view MCP servers'),
        ),
      );
    }

    if (state.mcpServers.isEmpty) {
      return const Center(
        child: Padding(
          padding: EdgeInsets.all(24),
          child: Text('No MCP servers found'),
        ),
      );
    }

    return ListView.separated(
      padding: const EdgeInsets.symmetric(vertical: 8),
      itemCount: state.mcpServers.length,
      separatorBuilder: (_, __) => const Divider(height: 0),
      itemBuilder: (context, index) {
        final server = state.mcpServers[index];
        return _McpServerTile(
          server: server,
          expanded: state.expandedMcpServers.contains(server.name),
          onExpansionChanged: (expanded) {
            if (expanded) {
              if (!state.expandedMcpServers.contains(server.name)) {
                controller.toggleMcpServerExpand(server.name);
              }
            } else if (state.expandedMcpServers.contains(server.name)) {
              controller.toggleMcpServerExpand(server.name);
            }
          },
        );
      },
    );
  }
}

class _McpServerTile extends StatelessWidget {
  const _McpServerTile({
    required this.server,
    required this.expanded,
    required this.onExpansionChanged,
  });

  final MCPServerInfo server;
  final bool expanded;
  final ValueChanged<bool> onExpansionChanged;

  @override
  Widget build(BuildContext context) {
    final isConnected = server.status == MCPServerStatusEnum.connected;
    final statusColor = isConnected ? Colors.green : Colors.red;
    final statusLabel = isConnected ? 'Connected' : 'Failed';
    final subtitle = isConnected
        ? '${server.toolCount} tools available'
        : (server.error ?? 'Connection failed');

    return ExpansionTile(
      key: PageStorageKey<String>(server.name),
      initiallyExpanded: expanded,
      onExpansionChanged: onExpansionChanged,
      title: Row(
        children: [
          Expanded(
            child: Text(
              server.name,
              style: const TextStyle(fontWeight: FontWeight.w600),
            ),
          ),
          Container(
            padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
            decoration: BoxDecoration(
              color: statusColor.withValues(alpha: 0.15),
              borderRadius: BorderRadius.circular(12),
            ),
            child: Text(
              statusLabel,
              style: TextStyle(
                color: statusColor,
                fontSize: 12,
                fontWeight: FontWeight.w600,
              ),
            ),
          ),
        ],
      ),
      subtitle: Text(subtitle),
      children: server.tools.isEmpty
          ? const [
              ListTile(
                dense: true,
                title: Text('No tools available'),
              ),
            ]
          : server.tools
              .map(
                (toolName) => ListTile(
                  dense: true,
                  leading: const Icon(Icons.build_outlined, size: 18),
                  title: Text(toolName),
                ),
              )
              .toList(),
    );
  }
}
