import 'package:flutter/material.dart';
import 'package:intl/intl.dart';

import '../../data/ws/ws_dto.dart';

class ConversationCard extends StatelessWidget {
  const ConversationCard({
    super.key,
    required this.summary,
    required this.isSelected,
    required this.onTap,
  });

  final ConversationSummary summary;
  final bool isSelected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final updated =
        DateTime.fromMillisecondsSinceEpoch(summary.updatedAt * 1000);
    final subtitle = summary.lastMessagePreview ?? 'Ready to help';
    final badge = DateFormat('HH:mm').format(updated);

    return ListTile(
      selected: isSelected,
      onTap: onTap,
      leading: const CircleAvatar(child: Icon(Icons.chat_bubble_outline)),
      title: Text(summary.title),
      subtitle: Text(subtitle, maxLines: 1, overflow: TextOverflow.ellipsis),
      trailing: Text(
        badge,
        style: Theme.of(context).textTheme.labelMedium,
      ),
    );
  }
}










