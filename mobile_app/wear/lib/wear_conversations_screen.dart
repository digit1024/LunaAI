import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:intl/intl.dart';

import 'package:luna_mobile/application/app_controller.dart';
import 'package:luna_mobile/data/ws/ws_dto.dart';

class WearConversationsScreen extends ConsumerStatefulWidget {
  const WearConversationsScreen({super.key});

  @override
  ConsumerState<WearConversationsScreen> createState() => _WearConversationsScreenState();
}

class _WearConversationsScreenState extends ConsumerState<WearConversationsScreen> {
  @override
  void initState() {
    super.initState();
    // Refresh conversations when screen opens
    WidgetsBinding.instance.addPostFrameCallback((_) {
      ref.read(appControllerProvider.notifier).refreshConversations();
    });
  }

  @override
  Widget build(BuildContext context) {
    final state = ref.watch(appControllerProvider);
    final controller = ref.read(appControllerProvider.notifier);

    return Scaffold(
      body: Column(
        children: [
          // Header
          Container(
            padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
            child: Row(
              children: [
                IconButton(
                  icon: const Icon(Icons.arrow_back, size: 18),
                  onPressed: () {
                    // Go back to chat
                    if (state.activeConversation != null) {
                      controller.selectConversation(state.activeConversation!.id);
                    } else {
                      controller.startNewConversation();
                    }
                  },
                  padding: EdgeInsets.zero,
                  constraints: const BoxConstraints(minWidth: 32, minHeight: 32),
                ),
                const Expanded(
                  child: Text(
                    'History',
                    style: TextStyle(
                      fontSize: 14,
                      fontWeight: FontWeight.bold,
                    ),
                    textAlign: TextAlign.center,
                  ),
                ),
                IconButton(
                  icon: const Icon(Icons.add, size: 18),
                  onPressed: () => controller.startNewConversation(),
                  padding: EdgeInsets.zero,
                  constraints: const BoxConstraints(minWidth: 32, minHeight: 32),
                ),
              ],
            ),
          ),
          // Conversation list
          Expanded(
            child: state.conversations.isEmpty
                ? Center(
                    child: Column(
                      mainAxisAlignment: MainAxisAlignment.center,
                      children: [
                        Icon(
                          Icons.chat_bubble_outline,
                          size: 32,
                          color: Theme.of(context).colorScheme.onSurface.withValues(alpha: 0.5),
                        ),
                        const SizedBox(height: 8),
                        Text(
                          'No conversations yet',
                          style: TextStyle(
                            fontSize: 12,
                            color: Theme.of(context).colorScheme.onSurface.withValues(alpha: 0.5),
                          ),
                        ),
                      ],
                    ),
                  )
                : ListView.builder(
                    padding: const EdgeInsets.symmetric(horizontal: 4),
                    itemCount: state.conversations.length,
                    itemBuilder: (context, index) {
                      final conversation = state.conversations[index];
                      return _WearConversationTile(
                        conversation: conversation,
                        isActive: state.activeConversation?.id == conversation.id,
                        onTap: () => controller.selectConversation(conversation.id),
                      );
                    },
                  ),
          ),
        ],
      ),
    );
  }
}

class _WearConversationTile extends StatelessWidget {
  const _WearConversationTile({
    required this.conversation,
    required this.isActive,
    required this.onTap,
  });

  final ConversationSummary conversation;
  final bool isActive;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final dateFormat = DateFormat('MM/dd HH:mm');

    return Card(
      margin: const EdgeInsets.symmetric(vertical: 2),
      color: isActive 
          ? theme.colorScheme.primaryContainer
          : theme.colorScheme.surface,
      child: InkWell(
        onTap: onTap,
        borderRadius: BorderRadius.circular(8),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                conversation.title.isNotEmpty 
                    ? conversation.title 
                    : 'Untitled',
                style: TextStyle(
                  fontSize: 12,
                  fontWeight: FontWeight.w500,
                  color: isActive 
                      ? theme.colorScheme.onPrimaryContainer
                      : theme.colorScheme.onSurface,
                ),
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
              ),
              const SizedBox(height: 2),
              Text(
                dateFormat.format(DateTime.fromMillisecondsSinceEpoch(conversation.updatedAt * 1000)),
                style: TextStyle(
                  fontSize: 10,
                  color: isActive 
                      ? theme.colorScheme.onPrimaryContainer.withValues(alpha: 0.7)
                      : theme.colorScheme.onSurface.withValues(alpha: 0.5),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

