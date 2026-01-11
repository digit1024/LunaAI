import 'package:flutter/material.dart';
import 'package:luna_mobile/application/app_state.dart';
import 'package:luna_mobile/utils/text_processing.dart';

class WearMessageBubble extends StatelessWidget {
  const WearMessageBubble({
    super.key,
    required this.message,
  });

  final ChatMessage message;

  @override
  Widget build(BuildContext context) {
    final isUser = message.bubbleType == BubbleType.user;
    final isAssistant = message.bubbleType == BubbleType.assistant;
    final isTool = message.bubbleType == BubbleType.toolRequest ||
        message.bubbleType == BubbleType.toolResult;

    if (isTool) {
      return _buildToolBubble(context);
    }

    final content = isAssistant
        ? stripEmojisAndMarkdown(message.content)
        : message.content;

    return Align(
      alignment: isUser ? Alignment.centerRight : Alignment.centerLeft,
      child: Container(
        constraints: const BoxConstraints(maxWidth: 200),
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
        decoration: BoxDecoration(
          color: isUser
              ? Theme.of(context).colorScheme.primary
              : Theme.of(context).colorScheme.surfaceContainerHighest,
          borderRadius: BorderRadius.circular(12),
        ),
        child: Text(
          content,
          style: TextStyle(
            fontSize: 11,
            color: isUser
                ? Theme.of(context).colorScheme.onPrimary
                : Theme.of(context).colorScheme.onSurfaceVariant,
          ),
          maxLines: 5,
          overflow: TextOverflow.ellipsis,
        ),
      ),
    );
  }

  Widget _buildToolBubble(BuildContext context) {
    final isRequest = message.bubbleType == BubbleType.toolRequest;
    final status = message.toolStatus ?? 'unknown';
    final toolName = message.toolName ?? 'Tool';

    Color statusColor;
    String statusText;
    switch (status) {
      case 'planned':
        statusColor = Colors.blue;
        statusText = 'Planned';
        break;
      case 'running':
        statusColor = Colors.orange;
        statusText = 'Running...';
        break;
      case 'done':
        statusColor = Colors.green;
        statusText = 'Done';
        break;
      case 'error':
        statusColor = Colors.red;
        statusText = 'Error';
        break;
      default:
        statusColor = Colors.grey;
        statusText = status;
    }

    return Align(
      alignment: Alignment.centerLeft,
      child: Container(
        constraints: const BoxConstraints(maxWidth: 200),
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
        decoration: BoxDecoration(
          color: Theme.of(context).colorScheme.surfaceContainerHighest,
          borderRadius: BorderRadius.circular(12),
          border: Border.all(
            color: statusColor.withValues(alpha: 0.5),
            width: 1,
          ),
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: [
            Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(
                  isRequest ? Icons.build : Icons.check_circle,
                  size: 12,
                  color: statusColor,
                ),
                const SizedBox(width: 4),
                Text(
                  toolName,
                  style: TextStyle(
                    fontSize: 10,
                    fontWeight: FontWeight.bold,
                    color: Theme.of(context).colorScheme.onSurface,
                  ),
                ),
              ],
            ),
            if (isRequest && message.content.isNotEmpty) ...[
              const SizedBox(height: 4),
              Text(
                message.content,
                style: TextStyle(
                  fontSize: 9,
                  color: Theme.of(context).colorScheme.onSurface.withValues(alpha: 0.7),
                ),
                maxLines: 2,
                overflow: TextOverflow.ellipsis,
              ),
            ],
            const SizedBox(height: 2),
            Text(
              statusText,
              style: TextStyle(
                fontSize: 9,
                color: statusColor,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

