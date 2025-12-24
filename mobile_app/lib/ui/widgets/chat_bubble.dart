import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_markdown_plus/flutter_markdown_plus.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../application/app_state.dart';
import '../../services/tts_service.dart';

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
    this.prevMessage,
    this.nextMessage,
  });

  final ChatMessage message;
  final ChatMessage? prevMessage;
  final ChatMessage? nextMessage;

  bool _isAssistantType(BubbleType? type) {
    if (type == null) return false;
    return type == BubbleType.assistant ||
        type == BubbleType.toolRequest ||
        type == BubbleType.toolResult;
  }

  @override
  Widget build(BuildContext context) {
    final isPrevUser = prevMessage?.bubbleType == BubbleType.user;
    final isPrevAssistant = _isAssistantType(prevMessage?.bubbleType);
    final isNextAssistant = _isAssistantType(nextMessage?.bubbleType);

    switch (message.bubbleType) {
      case BubbleType.user:
        return _SlideInWrapper(
          direction: SlideDirection.right,
          child: _UserBubble(message: message),
        );
      case BubbleType.assistant:
        return _SlideInWrapper(
          direction: SlideDirection.left,
          child: _AssistantBubble(
            message: message,
            isPrevUser: isPrevUser,
            isPrevAssistant: isPrevAssistant,
            isNextAssistant: isNextAssistant,
          ),
        );
      case BubbleType.toolRequest:
        return _SlideInWrapper(
          direction: SlideDirection.left,
          child: _ToolRequestBubble(
            message: message,
            isPrevUser: isPrevUser,
            isPrevAssistant: isPrevAssistant,
            isNextAssistant: isNextAssistant,
          ),
        );
      case BubbleType.toolResult:
        return _SlideInWrapper(
          direction: SlideDirection.left,
          child: _ToolResultBubble(
            message: message,
            isPrevUser: isPrevUser,
            isPrevAssistant: isPrevAssistant,
            isNextAssistant: isNextAssistant,
          ),
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
          child: Container(
            margin: const EdgeInsets.symmetric(vertical: 6),
            decoration: BoxDecoration(
              color: colorScheme.primaryContainer,
              borderRadius: BorderRadius.only(
                topLeft: const Radius.circular(18),
                topRight: const Radius.circular(18),
                bottomLeft: const Radius.circular(18),
                bottomRight: const Radius.circular(4), // Smaller right bottom corner
              ),
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
      ),
    );
  }
}

/// Assistant message bubble (left-aligned) with smart corners
class _AssistantBubble extends ConsumerWidget {
  const _AssistantBubble({
    required this.message,
    required this.isPrevUser,
    required this.isPrevAssistant,
    required this.isNextAssistant,
  });

  final ChatMessage message;
  final bool isPrevUser;
  final bool isPrevAssistant;
  final bool isNextAssistant;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final maxWidth = MediaQuery.of(context).size.width * 0.85;
    final colorScheme = Theme.of(context).colorScheme;

    // Smart corner rounding - 0 when adjacent to AI/tool messages
    final topLeftRadius = isPrevAssistant ? 0.0 : 18.0;
    final topRightRadius = isPrevAssistant ? 0.0 : 18.0;
    final bottomLeftRadius = isNextAssistant ? 0.0 : 18.0;
    final bottomRightRadius = isNextAssistant ? 0.0 : 18.0;

    return Align(
      alignment: Alignment.centerLeft,
      child: ConstrainedBox(
        constraints: BoxConstraints(maxWidth: maxWidth),
        child: GestureDetector(
          onLongPress: () => _copyToClipboard(context, message.content),
          child: Container(
            margin: EdgeInsets.only(
              top: isPrevAssistant ? 0 : 6,
              bottom: isNextAssistant ? 0 : 6,
            ),
            decoration: BoxDecoration(
              color: colorScheme.surfaceContainerHighest,
              borderRadius: BorderRadius.only(
                topLeft: Radius.circular(topLeftRadius),
                topRight: Radius.circular(topRightRadius),
                bottomLeft: Radius.circular(bottomLeftRadius),
                bottomRight: Radius.circular(bottomRightRadius),
              ),
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
                      initiallyExpanded: message.isStreaming,
                    ),
                  ],
                  const SizedBox(height: 6),
                  Row(
                    mainAxisAlignment: MainAxisAlignment.end,
                    children: [
                      Text(
                        _formatTimestamp(message.timestamp),
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
                        onPressed: () => _copyToClipboard(context, message.content),
                      ),
                      if (!message.isStreaming) ...[
                        const SizedBox(width: 4),
                        _TtsPlayButton(
                          text: message.content,
                          ref: ref,
                        ),
                      ],
                    ],
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

/// TTS Play button widget
class _TtsPlayButton extends ConsumerStatefulWidget {
  const _TtsPlayButton({
    required this.text,
    required this.ref,
  });

  final String text;
  final WidgetRef ref;

  @override
  ConsumerState<_TtsPlayButton> createState() => _TtsPlayButtonState();
}

class _TtsPlayButtonState extends ConsumerState<_TtsPlayButton> {
  bool _isPlaying = false;

  Future<void> _toggleTts() async {
    if (_isPlaying) {
      final ttsService = ref.read(ttsServiceProvider);
      await ttsService.stop();
      setState(() => _isPlaying = false);
    } else {
      final ttsService = ref.read(ttsServiceProvider);
      // Clean text (remove markdown)
      final cleanText = widget.text
          .replaceAll(RegExp(r'#+\s+'), '') // Remove headers
          .replaceAll(RegExp(r'\*\*(.*?)\*\*'), r'$1') // Remove bold
          .replaceAll(RegExp(r'\*(.*?)\*'), r'$1') // Remove italic
          .replaceAll(RegExp(r'`(.*?)`'), r'$1') // Remove code
          .replaceAll(RegExp(r'\[(.*?)\]\(.*?\)'), r'$1') // Remove links
          .trim();

      if (cleanText.isNotEmpty) {
        setState(() => _isPlaying = true);
        await ttsService.speak(cleanText, onComplete: () {
          if (mounted) {
            setState(() => _isPlaying = false);
          }
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final colorScheme = Theme.of(context).colorScheme;
    return IconButton(
      icon: Icon(_isPlaying ? Icons.stop : Icons.play_arrow),
      iconSize: 18,
      padding: EdgeInsets.zero,
      constraints: const BoxConstraints(),
      color: colorScheme.primary,
      tooltip: _isPlaying ? 'Stop reading' : 'Read aloud',
      onPressed: _toggleTts,
    );
  }
}

/// Tool request bubble - shows tool name, status, and parameters
class _ToolRequestBubble extends StatefulWidget {
  const _ToolRequestBubble({
    required this.message,
    required this.isPrevUser,
    required this.isPrevAssistant,
    required this.isNextAssistant,
  });

  final ChatMessage message;
  final bool isPrevUser;
  final bool isPrevAssistant;
  final bool isNextAssistant;

  @override
  State<_ToolRequestBubble> createState() => _ToolRequestBubbleState();
}

class _ToolRequestBubbleState extends State<_ToolRequestBubble> {
  bool _isExpanded = false;

  @override
  Widget build(BuildContext context) {
    final colorScheme = Theme.of(context).colorScheme;
    final paramsText = _stringify(widget.message.toolParams);

    // Smart corner rounding - 0 when adjacent to AI/tool messages
    final topLeftRadius = widget.isPrevAssistant ? 0.0 : 18.0;
    final topRightRadius = widget.isPrevAssistant ? 0.0 : 18.0;
    final bottomLeftRadius = widget.isNextAssistant ? 0.0 : 18.0;
    final bottomRightRadius = widget.isNextAssistant ? 0.0 : 18.0;

    return Align(
      alignment: Alignment.centerLeft,
      child: ConstrainedBox(
        constraints: BoxConstraints(
          maxWidth: MediaQuery.of(context).size.width * 0.85,
        ),
        child: Container(
          margin: EdgeInsets.only(
            top: widget.isPrevAssistant ? 0 : 6,
            bottom: widget.isNextAssistant ? 0 : 6,
          ),
          decoration: BoxDecoration(
            color: colorScheme.surfaceContainerHighest,
            borderRadius: BorderRadius.only(
              topLeft: Radius.circular(topLeftRadius),
              topRight: Radius.circular(topRightRadius),
              bottomLeft: Radius.circular(bottomLeftRadius),
              bottomRight: Radius.circular(bottomRightRadius),
            ),
          ),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                // One-line header
                Row(
                  children: [
                    Icon(
                      Icons.build_circle,
                      size: 18,
                      color: colorScheme.tertiary,
                    ),
                    const SizedBox(width: 8),
                    Expanded(
                      child: Text(
                        widget.message.toolName ?? 'Tool',
                        style: Theme.of(context).textTheme.bodyMedium?.copyWith(
                              fontWeight: FontWeight.w500,
                            ),
                        overflow: TextOverflow.ellipsis,
                      ),
                    ),
                    if (paramsText != null && paramsText.isNotEmpty)
                      IconButton(
                        icon: Icon(
                          _isExpanded ? Icons.expand_less : Icons.expand_more,
                          size: 20,
                        ),
                        padding: EdgeInsets.zero,
                        constraints: const BoxConstraints(),
                        onPressed: () {
                          setState(() {
                            _isExpanded = !_isExpanded;
                          });
                        },
                        tooltip: _isExpanded ? 'Collapse' : 'Expand',
                      ),
                  ],
                ),

                // Parameters (expandable)
                if (_isExpanded && paramsText != null && paramsText.isNotEmpty) ...[
                  const SizedBox(height: 8),
                  Container(
                    width: double.infinity,
                    padding: const EdgeInsets.all(8),
                    decoration: BoxDecoration(
                      color: colorScheme.surfaceContainerHighest.withOpacity(0.4),
                      borderRadius: BorderRadius.circular(8),
                    ),
                    child: SelectableText(
                      paramsText,
                      style: Theme.of(context).textTheme.bodySmall?.copyWith(
                            fontFamily: 'monospace',
                          ),
                    ),
                  ),
                ],
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
        child: Container(
          margin: const EdgeInsets.symmetric(vertical: 6),
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
      ),
    );
  }
}

/// Tool result bubble - shows tool result or error
class _ToolResultBubble extends StatefulWidget {
  const _ToolResultBubble({
    required this.message,
    required this.isPrevUser,
    required this.isPrevAssistant,
    required this.isNextAssistant,
  });

  final ChatMessage message;
  final bool isPrevUser;
  final bool isPrevAssistant;
  final bool isNextAssistant;

  @override
  State<_ToolResultBubble> createState() => _ToolResultBubbleState();
}

class _ToolResultBubbleState extends State<_ToolResultBubble> {
  bool _isExpanded = false;

  bool get isError => widget.message.toolStatus == 'error' || widget.message.toolError != null;

  @override
  Widget build(BuildContext context) {
    final colorScheme = Theme.of(context).colorScheme;
    final resultText = _stringify(widget.message.toolResult);
    final errorText = widget.message.toolError;

    // Smart corner rounding - 0 when adjacent to AI/tool messages
    final topLeftRadius = widget.isPrevAssistant ? 0.0 : 18.0;
    final topRightRadius = widget.isPrevAssistant ? 0.0 : 18.0;
    final bottomLeftRadius = widget.isNextAssistant ? 0.0 : 18.0;
    final bottomRightRadius = widget.isNextAssistant ? 0.0 : 18.0;

    final hasContent = (resultText != null && resultText.isNotEmpty) ||
        (errorText != null && errorText.isNotEmpty);

    return Align(
      alignment: Alignment.centerLeft,
      child: ConstrainedBox(
        constraints: BoxConstraints(
          maxWidth: MediaQuery.of(context).size.width * 0.85,
        ),
        child: Container(
          margin: EdgeInsets.only(
            top: widget.isPrevAssistant ? 0 : 6,
            bottom: widget.isNextAssistant ? 0 : 6,
          ),
          decoration: BoxDecoration(
            color: colorScheme.surfaceContainerHighest,
            borderRadius: BorderRadius.only(
              topLeft: Radius.circular(topLeftRadius),
              topRight: Radius.circular(topRightRadius),
              bottomLeft: Radius.circular(bottomLeftRadius),
              bottomRight: Radius.circular(bottomRightRadius),
            ),
          ),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                // One-line header
                Row(
                  children: [
                    Icon(
                      isError ? Icons.error : Icons.check_circle,
                      size: 18,
                      color: isError ? colorScheme.error : Colors.green,
                    ),
                    const SizedBox(width: 8),
                    Expanded(
                      child: Text(
                        '${widget.message.toolName ?? 'Tool'} Result',
                        style: Theme.of(context).textTheme.bodyMedium?.copyWith(
                              fontWeight: FontWeight.w500,
                            ),
                        overflow: TextOverflow.ellipsis,
                      ),
                    ),
                    if (hasContent)
                      IconButton(
                        icon: Icon(
                          _isExpanded ? Icons.expand_less : Icons.expand_more,
                          size: 20,
                        ),
                        padding: EdgeInsets.zero,
                        constraints: const BoxConstraints(),
                        onPressed: () {
                          setState(() {
                            _isExpanded = !_isExpanded;
                          });
                        },
                        tooltip: _isExpanded ? 'Collapse' : 'Expand',
                      ),
                  ],
                ),

                // Error message (always shown if exists)
                if (_isExpanded && errorText != null && errorText.isNotEmpty) ...[
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

                // Result (expandable)
                if (_isExpanded && resultText != null && resultText.isNotEmpty) ...[
                  const SizedBox(height: 8),
                  Container(
                    width: double.infinity,
                    padding: const EdgeInsets.all(8),
                    decoration: BoxDecoration(
                      color: colorScheme.surfaceContainerHighest.withOpacity(0.4),
                      borderRadius: BorderRadius.circular(8),
                    ),
                    child: SelectableText(
                      resultText,
                      style: Theme.of(context).textTheme.bodySmall?.copyWith(
                            fontFamily: 'monospace',
                          ),
                    ),
                  ),
                ],
              ],
            ),
          ),
        ),
      ),
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
