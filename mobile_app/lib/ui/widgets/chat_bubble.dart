import 'dart:convert';

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
    final bubbleColor =
        isUser ? colorScheme.primaryContainer : colorScheme.surfaceVariant;
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
                if (message.toolChip != null)
                  _ToolChip(chip: message.toolChip!),
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

  String? _stringify(dynamic data) {
    if (data == null) return null;
    if (data is String) return data;
    try {
      return const JsonEncoder.withIndent('  ').convert(data);
    } catch (_) {
      return data.toString();
    }
  }

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
    final paramsText = _stringify(chip.params);
    final resultText = _stringify(chip.result);
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
                padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
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
          if (chip.error?.isNotEmpty == true) ...[
            const SizedBox(height: 6),
            Text(
              chip.error!,
              style: Theme.of(context)
                  .textTheme
                  .bodySmall
                  ?.copyWith(color: Theme.of(context).colorScheme.error),
            ),
          ],
          if (paramsText != null && paramsText.isNotEmpty) ...[
            const SizedBox(height: 6),
            _CollapsiblePayload(
              icon: Icons.tune,
              label: 'Parameters',
              payload: paramsText,
            ),
          ],
          if (resultText != null && resultText.isNotEmpty) ...[
            const SizedBox(height: 6),
            _CollapsiblePayload(
              icon: Icons.summarize,
              label: 'Result',
              payload: resultText,
            ),
          ],
        ],
      ),
    );
  }
}

class _CollapsiblePayload extends StatelessWidget {
  const _CollapsiblePayload({
    required this.icon,
    required this.label,
    required this.payload,
  });

  final IconData icon;
  final String label;
  final String payload;

  @override
  Widget build(BuildContext context) {
    final colorScheme = Theme.of(context).colorScheme;
    final tileTheme = Theme.of(context).copyWith(
      dividerColor: Colors.transparent,
      splashColor: Colors.transparent,
      hoverColor: Colors.transparent,
    );

    return Theme(
      data: tileTheme,
      child: Container(
        decoration: BoxDecoration(
          borderRadius: BorderRadius.circular(10),
          border: Border.all(color: colorScheme.outlineVariant),
        ),
        child: ExpansionTile(
          dense: true,
          tilePadding: const EdgeInsets.symmetric(horizontal: 12),
          childrenPadding:
              const EdgeInsets.only(left: 16, right: 12, bottom: 12),
          visualDensity: VisualDensity.compact,
          leading: Icon(icon, size: 18, color: colorScheme.primary),
          title: Text(
            label,
            style: Theme.of(context).textTheme.bodyMedium,
          ),
          children: [
            Container(
              width: double.infinity,
              padding: const EdgeInsets.all(8),
              decoration: BoxDecoration(
                color: colorScheme.surfaceVariant.withOpacity(0.4),
                borderRadius: BorderRadius.circular(8),
              ),
              child: SelectableText(
                payload,
                style: Theme.of(context).textTheme.bodySmall?.copyWith(
                      fontFamily: 'monospace',
                    ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
