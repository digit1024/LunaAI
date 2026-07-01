import 'package:flutter/material.dart';
import 'package:intl/intl.dart';

import '../../data/ws/ws_dto.dart';
import 'swipe_list_tile.dart';

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
    final colorScheme = Theme.of(context).colorScheme;

    final startActions = <SwipeAction>[
      if (onEdit != null)
        SwipeAction(
          icon: Icons.edit_outlined,
          label: 'Rename',
          backgroundColor: colorScheme.primary,
          onPressed: onEdit!,
        ),
      if (onToggleTransient != null)
        SwipeAction(
          icon: summary.internal
              ? Icons.visibility_outlined
              : Icons.visibility_off_outlined,
          label: summary.internal ? 'Visible' : 'Internal',
          backgroundColor: colorScheme.tertiary,
          onPressed: onToggleTransient!,
        ),
    ];

    final endActions = <SwipeAction>[
      if (onDelete != null)
        SwipeAction(
          icon: Icons.delete_outline,
          label: 'Delete',
          backgroundColor: colorScheme.error,
          onPressed: onDelete!,
        ),
    ];

    return SwipeListTile(
      slidableKey: ValueKey('conversation-${summary.id}'),
      startActions: startActions,
      endActions: endActions,
      child: ListTile(
        selected: isSelected,
        onTap: onTap,
        leading: CircleAvatar(
          child: Icon(
            summary.internal
                ? Icons.visibility_off_outlined
                : Icons.chat_bubble_outline,
          ),
        ),
        title: Row(
          children: [
            Expanded(child: Text(summary.title)),
            if (summary.internal)
              Padding(
                padding: const EdgeInsets.only(left: 8),
                child: Text(
                  'internal',
                  style: Theme.of(context).textTheme.labelSmall,
                ),
              ),
          ],
        ),
        subtitle: Text(subtitle, maxLines: 1, overflow: TextOverflow.ellipsis),
        trailing: Text(
          badge,
          style: Theme.of(context).textTheme.labelMedium,
        ),
      ),
    );
  }
}
