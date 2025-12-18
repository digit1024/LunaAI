import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_markdown_plus/flutter_markdown_plus.dart';

import '../../application/app_state.dart';

/// Slide direction for bubble animations
enum SlideDirection { left, right, center }

/// Animated wrapper that slides bubbles in from left/right
class _SlideInWrapper extends StatefulWidget {
  const _SlideInWrapper({
    required this.child,
    required this.direction,
  });

  final Widget child;
  final SlideDirection direction;

  @override
  State<_SlideInWrapper> createState() => _SlideInWrapperState();
}

class _SlideInWrapperState extends State<_SlideInWrapper>
    with SingleTickerProviderStateMixin {
  late AnimationController _controller;
  late Animation<Offset> _slideAnimation;
  late Animation<double> _fadeAnimation;

  @override
  void initState() {
    super.initState();
    _controller = AnimationController(
      duration: const Duration(milliseconds: 300),
      vsync: this,
    );

    // Slide offset based on direction
    final beginOffset = switch (widget.direction) {
      SlideDirection.left => const Offset(-0.3, 0),
      SlideDirection.right => const Offset(0.3, 0),
      SlideDirection.center => Offset.zero,
    };

    _slideAnimation = Tween<Offset>(
      begin: beginOffset,
      end: Offset.zero,
    ).animate(CurvedAnimation(
      parent: _controller,
      curve: Curves.easeOutCubic,
    ));

    _fadeAnimation = Tween<double>(
      begin: 0.0,
      end: 1.0,
    ).animate(CurvedAnimation(
      parent: _controller,
      curve: Curves.easeOut,
    ));

    _controller.forward();
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return SlideTransition(
      position: _slideAnimation,
      child: FadeTransition(
        opacity: _fadeAnimation,
        child: widget.child,
      ),
    );
  }
}

/// Main chat bubble widget - routes to appropriate bubble type
class ChatBubble extends StatelessWidget {
  const ChatBubble({
    super.key,
    required this.message,
  });

  final ChatMessage message;

  @override
  Widget build(BuildContext context) {
    switch (message.bubbleType) {
      case BubbleType.user:
        return _SlideInWrapper(
          direction: SlideDirection.right,
          child: _UserBubble(message: message),
        );
      case BubbleType.assistant:
        return _SlideInWrapper(
          direction: SlideDirection.left,
          child: _AssistantBubble(message: message),
        );
      case BubbleType.toolRequest:
        return _SlideInWrapper(
          direction: SlideDirection.left,
          child: _ToolRequestBubble(message: message),
        );
      case BubbleType.toolResult:
        return _SlideInWrapper(
          direction: SlideDirection.left,
          child: _ToolResultBubble(message: message),
        );
      case BubbleType.summary:
        return _SlideInWrapper(
          direction: SlideDirection.center,
          child: _SummaryBubble(message: message),
        );
    }
  }
}

/// User message bubble (right-aligned)
class _UserBubble extends StatelessWidget {
  const _UserBubble({required this.message});

  final ChatMessage message;

  @override
  Widget build(BuildContext context) {
    final maxWidth = MediaQuery.of(context).size.width * 0.7;
    final colorScheme = Theme.of(context).colorScheme;

    return Align(
      alignment: Alignment.centerRight,
      child: ConstrainedBox(
        constraints: BoxConstraints(maxWidth: maxWidth),
        child: GestureDetector(
          onLongPress: () => _copyToClipboard(context, message.content),
          child: Card(
            color: colorScheme.primaryContainer,
            shape: RoundedRectangleBorder(
              borderRadius: BorderRadius.circular(18),
            ),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.end,
                children: [
                  SelectableText(
                    message.content,
                    style: Theme.of(context).textTheme.bodyMedium,
                  ),
                  const SizedBox(height: 6),
                  _TimestampRow(
                    timestamp: message.timestamp,
                    onCopy: () => _copyToClipboard(context, message.content),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

/// Assistant message bubble (left-aligned)
class _AssistantBubble extends StatelessWidget {
  const _AssistantBubble({required this.message});

  final ChatMessage message;

  @override
  Widget build(BuildContext context) {
    final maxWidth = MediaQuery.of(context).size.width * 0.7;
    final colorScheme = Theme.of(context).colorScheme;

    return Align(
      alignment: Alignment.centerLeft,
      child: ConstrainedBox(
        constraints: BoxConstraints(maxWidth: maxWidth),
        child: GestureDetector(
          onLongPress: () => _copyToClipboard(context, message.content),
          child: Card(
            color: colorScheme.surfaceContainerHighest,
            shape: RoundedRectangleBorder(
              borderRadius: BorderRadius.circular(18),
            ),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  if (message.isStreaming)
                    SelectableText(
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
                  // Reasoning content (collapsible) - show during streaming too
                  if (message.reasoningContent != null && 
                      message.reasoningContent!.isNotEmpty) ...[
                    const SizedBox(height: 8),
                    _CollapsiblePayload(
                      icon: Icons.psychology,
                      label: '💭 Thinking',
                      payload: message.reasoningContent!,
                      initiallyExpanded: message.isStreaming, // Expand during streaming
                    ),
                  ],
                  const SizedBox(height: 6),
                  _TimestampRow(
                    timestamp: message.timestamp,
                    onCopy: () => _copyToClipboard(context, message.content),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

/// Tool request bubble - shows tool name, status, and parameters
class _ToolRequestBubble extends StatelessWidget {
  const _ToolRequestBubble({required this.message});

  final ChatMessage message;

  Color _statusColor(BuildContext context) {
    switch (message.toolStatus) {
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

  IconData _statusIcon() {
    switch (message.toolStatus) {
      case 'done':
        return Icons.check_circle;
      case 'error':
        return Icons.error;
      case 'running':
        return Icons.hourglass_top;
      default:
        return Icons.schedule;
    }
  }

  @override
  Widget build(BuildContext context) {
    final colorScheme = Theme.of(context).colorScheme;
    final paramsText = _stringify(message.toolParams);

    return Align(
      alignment: Alignment.centerLeft,
      child: ConstrainedBox(
        constraints: BoxConstraints(
          maxWidth: MediaQuery.of(context).size.width * 0.85,
        ),
        child: Card(
          color: colorScheme.tertiaryContainer.withOpacity(0.6),
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(14),
            side: BorderSide(
              color: _statusColor(context).withOpacity(0.4),
              width: 1,
            ),
          ),
          child: Padding(
            padding: const EdgeInsets.all(12),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                // Header row
                Row(
                  children: [
                    Icon(
                      Icons.build_circle,
                      size: 20,
                      color: colorScheme.tertiary,
                    ),
                    const SizedBox(width: 8),
                    Expanded(
                      child: Text(
                        '📤 ${message.toolName ?? 'Tool'}',
                        style: Theme.of(context).textTheme.titleSmall?.copyWith(
                              fontWeight: FontWeight.bold,
                            ),
                      ),
                    ),
                    _StatusChip(
                      status: message.toolStatus ?? 'planned',
                      color: _statusColor(context),
                      icon: _statusIcon(),
                    ),
                  ],
                ),
                
                // Parameters (collapsible)
                if (paramsText != null && paramsText.isNotEmpty) ...[
                  const SizedBox(height: 8),
                  _CollapsiblePayload(
                    icon: Icons.tune,
                    label: 'Parameters',
                    payload: paramsText,
                    initiallyExpanded: message.toolStatus == 'running',
                  ),
                ],
                
                const SizedBox(height: 6),
                Text(
                  _formatTimestamp(message.timestamp),
                  style: Theme.of(context).textTheme.labelSmall?.copyWith(
                        color: colorScheme.onSurfaceVariant,
                      ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// Summary bubble - shows summary of previous messages (collapsed by default)
class _SummaryBubble extends StatelessWidget {
  const _SummaryBubble({required this.message});

  final ChatMessage message;

  @override
  Widget build(BuildContext context) {
    final colorScheme = Theme.of(context).colorScheme;
    final summaryCount = message.summarizedCount ?? 0;

    return Align(
      alignment: Alignment.center,
      child: ConstrainedBox(
        constraints: BoxConstraints(
          maxWidth: MediaQuery.of(context).size.width * 0.95,
        ),
        child: Card(
          color: colorScheme.surfaceContainerHighest.withOpacity(0.7),
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(14),
            side: BorderSide(
              color: colorScheme.primary.withOpacity(0.3),
              width: 1,
            ),
          ),
          child: Padding(
            padding: const EdgeInsets.all(12),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                // Header row with toggle
                _CollapsiblePayload(
                  icon: Icons.summarize,
                  label: '📄 Summary ($summaryCount messages)',
                  payload: message.content,
                  initiallyExpanded: false,
                ),
                const SizedBox(height: 6),
                Text(
                  _formatTimestamp(message.timestamp),
                  style: Theme.of(context).textTheme.labelSmall?.copyWith(
                        color: colorScheme.onSurfaceVariant,
                      ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// Tool result bubble - shows tool result or error
class _ToolResultBubble extends StatelessWidget {
  const _ToolResultBubble({required this.message});

  final ChatMessage message;

  bool get isError => message.toolStatus == 'error' || message.toolError != null;

  @override
  Widget build(BuildContext context) {
    final colorScheme = Theme.of(context).colorScheme;
    final resultText = _stringify(message.toolResult);
    final errorText = message.toolError;

    return Align(
      alignment: Alignment.centerLeft,
      child: ConstrainedBox(
        constraints: BoxConstraints(
          maxWidth: MediaQuery.of(context).size.width * 0.85,
        ),
        child: Card(
          color: isError
              ? colorScheme.errorContainer.withOpacity(0.6)
              : colorScheme.secondaryContainer.withOpacity(0.6),
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(14),
            side: BorderSide(
              color: isError
                  ? colorScheme.error.withOpacity(0.4)
                  : Colors.green.withOpacity(0.4),
              width: 1,
            ),
          ),
          child: Padding(
            padding: const EdgeInsets.all(12),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                // Header row
                Row(
                  children: [
                    Icon(
                      isError ? Icons.error : Icons.check_circle,
                      size: 20,
                      color: isError ? colorScheme.error : Colors.green,
                    ),
                    const SizedBox(width: 8),
                    Expanded(
                      child: Text(
                        '📥 ${message.toolName ?? 'Tool'} Result',
                        style: Theme.of(context).textTheme.titleSmall?.copyWith(
                              fontWeight: FontWeight.bold,
                            ),
                      ),
                    ),
                    _StatusChip(
                      status: isError ? 'ERROR' : 'DONE',
                      color: isError ? colorScheme.error : Colors.green,
                      icon: isError ? Icons.close : Icons.check,
                    ),
                  ],
                ),
                
                // Error message
                if (errorText != null && errorText.isNotEmpty) ...[
                  const SizedBox(height: 8),
                  Container(
                    padding: const EdgeInsets.all(8),
                    decoration: BoxDecoration(
                      color: colorScheme.error.withOpacity(0.1),
                      borderRadius: BorderRadius.circular(8),
                    ),
                    child: Row(
                      children: [
                        Icon(Icons.warning, size: 16, color: colorScheme.error),
                        const SizedBox(width: 8),
                        Expanded(
                          child: Text(
                            errorText,
                            style: Theme.of(context).textTheme.bodySmall?.copyWith(
                                  color: colorScheme.error,
                                ),
                          ),
                        ),
                      ],
                    ),
                  ),
                ],
                
                // Result (collapsible)
                if (resultText != null && resultText.isNotEmpty) ...[
                  const SizedBox(height: 8),
                  _CollapsiblePayload(
                    icon: Icons.summarize,
                    label: 'Result',
                    payload: resultText,
                    initiallyExpanded: false,
                  ),
                ],
                
                const SizedBox(height: 6),
                Text(
                  _formatTimestamp(message.timestamp),
                  style: Theme.of(context).textTheme.labelSmall?.copyWith(
                        color: colorScheme.onSurfaceVariant,
                      ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// Status chip widget
class _StatusChip extends StatelessWidget {
  const _StatusChip({
    required this.status,
    required this.color,
    required this.icon,
  });

  final String status;
  final Color color;
  final IconData icon;

  @override
  Widget build(BuildContext context) {
    return Container(
      decoration: BoxDecoration(
        color: color.withOpacity(0.2),
        borderRadius: BorderRadius.circular(12),
      ),
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(icon, size: 12, color: color),
          const SizedBox(width: 4),
          Text(
            status.toUpperCase(),
            style: Theme.of(context).textTheme.labelSmall?.copyWith(
                  color: color,
                  fontWeight: FontWeight.bold,
                ),
          ),
        ],
      ),
    );
  }
}

/// Timestamp row with copy button
class _TimestampRow extends StatelessWidget {
  const _TimestampRow({
    required this.timestamp,
    required this.onCopy,
  });

  final DateTime timestamp;
  final VoidCallback onCopy;

  @override
  Widget build(BuildContext context) {
    final colorScheme = Theme.of(context).colorScheme;
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Text(
          _formatTimestamp(timestamp),
          style: Theme.of(context).textTheme.labelSmall?.copyWith(
                color: colorScheme.onSurfaceVariant,
              ),
        ),
        const SizedBox(width: 8),
        IconButton(
          icon: const Icon(Icons.copy, size: 16),
          padding: EdgeInsets.zero,
          constraints: const BoxConstraints(),
          tooltip: 'Copy message',
          onPressed: onCopy,
        ),
      ],
    );
  }
}

/// Collapsible payload section for params/results
class _CollapsiblePayload extends StatelessWidget {
  const _CollapsiblePayload({
    required this.icon,
    required this.label,
    required this.payload,
    this.initiallyExpanded = false,
  });

  final IconData icon;
  final String label;
  final String payload;
  final bool initiallyExpanded;

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
          initiallyExpanded: initiallyExpanded,
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
                color: colorScheme.surfaceContainerHighest.withOpacity(0.4),
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

// Helper functions
String _formatTimestamp(DateTime timestamp) {
  final timeOfDay = TimeOfDay.fromDateTime(timestamp);
  final minutes = timeOfDay.minute.toString().padLeft(2, '0');
  final hours = timeOfDay.hour.toString().padLeft(2, '0');
  return '$hours:$minutes';
}

String? _stringify(dynamic data) {
  if (data == null) return null;
  if (data is String) return data;
  try {
    return const JsonEncoder.withIndent('  ').convert(data);
  } catch (_) {
    return data.toString();
  }
}

void _copyToClipboard(BuildContext context, String text) {
  Clipboard.setData(ClipboardData(text: text));
  ScaffoldMessenger.of(context).showSnackBar(
    const SnackBar(
      content: Text('Message copied to clipboard'),
      duration: Duration(seconds: 2),
    ),
  );
}
