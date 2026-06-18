import 'package:flutter/material.dart';

class LunaBottomBar extends StatelessWidget {
  const LunaBottomBar({
    super.key,
    required this.onConversations,
    required this.onStartNew,
    required this.onMcpServers,
    required this.onSettings,
  });

  final VoidCallback onConversations;
  final VoidCallback onStartNew;
  final VoidCallback onMcpServers;
  final VoidCallback onSettings;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
      decoration: BoxDecoration(
        color: Theme.of(context).colorScheme.surface,
        border: Border(
          top: BorderSide(
            color: Theme.of(context).dividerColor,
          ),
        ),
      ),
      child: Row(
        children: [
Expanded(
  child: IconButton(
    onPressed: onConversations,
    icon: const Icon(Icons.chat_bubble_outline),
    tooltip: 'Conversations',
  ),
),
Expanded(
  child: IconButton(
    onPressed: onStartNew,
    icon: const Icon(Icons.add),
    tooltip: 'Start New',
  ),
),
Expanded(
  child: IconButton(
    onPressed: onMcpServers,
    icon: const Icon(Icons.hub_outlined),
    tooltip: 'MCP Servers',
  ),
),
Expanded(
  child: IconButton(
    onPressed: onSettings,
    icon: const Icon(Icons.settings),
    tooltip: 'Settings',
  ),
),
        ],
      ),
    );
  }
}










