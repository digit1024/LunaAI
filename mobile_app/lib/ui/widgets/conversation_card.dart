import 'package:flutter/material.dart';
import 'package:intl/intl.dart';

import '../../data/ws/ws_dto.dart';

class ConversationCard extends StatelessWidget {
  const ConversationCard({
    super.key,
    required this.summary,
    required this.isSelected,
    required this.onTap,
    this.onEdit,
    this.onDelete,
    this.onToggleTransient,
  });

  final ConversationSummary summary;
  final bool isSelected;
  final VoidCallback onTap;
  final VoidCallback? onEdit;
  final VoidCallback? onDelete;
  final VoidCallback? onToggleTransient;

  @override
  Widget build(BuildContext context) {
    final updated =
        DateTime.fromMillisecondsSinceEpoch(summary.updatedAt * 1000);
    final subtitle = summary.lastMessagePreview ?? 'Ready to help';
    final badge = DateFormat('HH:mm').format(updated);

    return ListTile(
      selected: isSelected,
      onTap: onTap,
      leading: CircleAvatar(
        child: Icon(
          summary.internal ? Icons.visibility_off_outlined : Icons.chat_bubble_outline,
        ),
      ),
      title: Row(
        children: [
          Expanded(child: Text(summary.title)),
          if (summary.internal)
            Padding(
              padding: const EdgeInsets.only(left: 8),
              child: Text(
                'transient',
                style: Theme.of(context).textTheme.labelSmall,
              ),
            ),
        ],
      ),
      subtitle: Text(subtitle, maxLines: 1, overflow: TextOverflow.ellipsis),
      trailing: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (onToggleTransient != null)
            IconButton(
              icon: Icon(
                summary.internal
                    ? Icons.visibility_outlined
                    : Icons.visibility_off_outlined,
                size: 20,
              ),
              tooltip: summary.internal ? 'Remove transient' : 'Mark transient',
              onPressed: onToggleTransient,
            ),
            IconButton(
              icon: const Icon(Icons.edit_outlined, size: 20),
              tooltip: 'Rename',
              onPressed: onEdit,
            ),
          if (onDelete != null)
            IconButton(
              icon: const Icon(Icons.delete_outline, size: 20),
              tooltip: 'Delete',
              onPressed: onDelete,
            ),
          Text(
            badge,
            style: Theme.of(context).textTheme.labelMedium,
          ),
        ],
      ),
    );
  }
}










