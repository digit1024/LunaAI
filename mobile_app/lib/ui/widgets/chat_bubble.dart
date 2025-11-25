import 'package:flutter/material.dart';
import 'package:flutter_markdown/flutter_markdown.dart';

import '../../application/app_state.dart';

class ChatBubble extends StatelessWidget {
  const ChatBubble({
    super.key,
    required this.message,
    required this.isUser,
  });

  final ChatMessage message;
  final bool isUser;

  @override
  Widget build(BuildContext context) {
    final maxWidth = MediaQuery.of(context).size.width * 0.7;
    final colorScheme = Theme.of(context).colorScheme;
    final bubbleColor = isUser
        ? colorScheme.primaryContainer
        : colorScheme.surfaceVariant;
    final alignment =
        isUser ? CrossAxisAlignment.end : CrossAxisAlignment.start;

    return Align(
      alignment: isUser ? Alignment.centerRight : Alignment.centerLeft,
      child: ConstrainedBox(
        constraints: BoxConstraints(maxWidth: maxWidth),
        child: Card(
          color: bubbleColor,
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(18),
          ),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
            child: Column(
              crossAxisAlignment: alignment,
              children: [
                if (message.toolChip != null) _ToolChip(chip: message.toolChip!),
                if (isUser || message.isStreaming)
                  Text(
                    message.content,
                    style: Theme.of(context).textTheme.bodyMedium,
                  )
                else
                  MarkdownBody(
                    data: message.content,
                    shrinkWrap: true,
                    styleSheet: MarkdownStyleSheet(
                      p: Theme.of(context).textTheme.bodyMedium,
                    ),
                  ),
                const SizedBox(height: 6),
                Text(
                  _formatTimestamp(message.timestamp),
                  style: Theme.of(context)
                      .textTheme
                      .labelSmall
                      ?.copyWith(color: colorScheme.onSurfaceVariant),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }

  String _formatTimestamp(DateTime timestamp) {
    final timeOfDay = TimeOfDay.fromDateTime(timestamp);
    final minutes = timeOfDay.minute.toString().padLeft(2, '0');
    final hours = timeOfDay.hour.toString().padLeft(2, '0');
    return '$hours:$minutes';
  }
}

class _ToolChip extends StatelessWidget {
  const _ToolChip({required this.chip});

  final ToolCallChip chip;

  Color _statusColor(BuildContext context) {
    switch (chip.status) {
      case 'done':
        return Colors.green;
      case 'error':
        return Theme.of(context).colorScheme.error;
      case 'running':
        return Colors.orange;
      default:
        return Theme.of(context).colorScheme.primary;
    }
  }

  @override
  Widget build(BuildContext context) {
    return Container(
      margin: const EdgeInsets.only(bottom: 8),
      padding: const EdgeInsets.all(8),
      decoration: BoxDecoration(
        color: Theme.of(context).colorScheme.surface,
        borderRadius: BorderRadius.circular(12),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Text('🧰 ${chip.name}'),
              const SizedBox(width: 6),
              Container(
                decoration: BoxDecoration(
                  color: _statusColor(context).withOpacity(0.2),
                  borderRadius: BorderRadius.circular(12),
                ),
                padding:
                    const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
                child: Text(
                  chip.status.toUpperCase(),
                  style: Theme.of(context).textTheme.labelSmall?.copyWith(
                        color: _statusColor(context),
                        fontWeight: FontWeight.bold,
                      ),
                ),
              ),
            ],
          ),
          if (chip.description.isNotEmpty) ...[
            const SizedBox(height: 4),
            Text(
              chip.description,
              style: Theme.of(context).textTheme.bodySmall,
            ),
          ],
        ],
      ),
    );
  }
}


