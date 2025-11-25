import 'package:flutter/material.dart';

class LunaBottomBar extends StatelessWidget {
  const LunaBottomBar({
    super.key,
    required this.onConversations,
    required this.onStartNew,
    required this.onSettings,
  });

  final VoidCallback onConversations;
  final VoidCallback onStartNew;
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
            child: TextButton.icon(
              onPressed: onConversations,
              icon: const Icon(Icons.chat_bubble_outline),
              label: const Text('Conversations'),
            ),
          ),
          Expanded(
            child: TextButton.icon(
              onPressed: onStartNew,
              icon: const Icon(Icons.add),
              label: const Text('+ Start New'),
            ),
          ),
          Expanded(
            child: TextButton.icon(
              onPressed: onSettings,
              icon: const Icon(Icons.settings),
              label: const Text('⚙ Settings'),
            ),
          ),
        ],
      ),
    );
  }
}


