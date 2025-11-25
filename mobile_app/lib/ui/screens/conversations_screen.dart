import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../application/app_controller.dart';
import '../../application/app_state.dart';
import '../../data/ws/ws_dto.dart';
import '../widgets/bottom_nav.dart';
import '../widgets/conversation_card.dart';

class ConversationsScreen extends ConsumerWidget {
  const ConversationsScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final state = ref.watch(appControllerProvider);
    final controller = ref.read(appControllerProvider.notifier);

    return Column(
      children: [
        _Header(
          status: state.connection,
          onRefresh: controller.refreshConversations,
        ),
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16),
          child: TextField(
            decoration: const InputDecoration(
              prefixIcon: Icon(Icons.search),
              hintText: 'Search history…',
            ),
            onSubmitted: controller.search,
          ),
        ),
        Expanded(
          child: _items(state).isEmpty
              ? const _EmptyPlaceholder()
              : ListView.separated(
                  itemCount: _items(state).length,
                  separatorBuilder: (_, __) => const Divider(height: 0),
                  itemBuilder: (context, index) {
                    final entry = _items(state)[index];
                    return entry.map(
                      summary: (summary) {
                        final selected =
                            state.activeConversation?.id == summary.id;
                        return ConversationCard(
                          summary: summary,
                          isSelected: selected,
                          onTap: () => controller.selectConversation(summary.id),
                        );
                      },
                      searchResult: (result) => ListTile(
                        leading: const Icon(Icons.history),
                        title: Text(result.snippet),
                        subtitle: Text('Conversation ${result.conversationId}'),
                        onTap: () =>
                            controller.selectConversation(result.conversationId),
                      ),
                    );
                  },
                ),
        ),
        LunaBottomBar(
          onConversations: controller.openConversations,
          onStartNew: controller.startNewConversation,
          onSettings: () =>
              ScaffoldMessenger.of(context).showSnackBar(const SnackBar(
            content: Text('Open Settings from the chat view'),
          )),
        ),
      ],
    );
  }
}

class _Header extends StatelessWidget {
  const _Header({required this.status, required this.onRefresh});

  final ConnectionStatus status;
  final VoidCallback onRefresh;

  @override
  Widget build(BuildContext context) {
    final indicator = switch (status) {
      ConnectionStatus.connecting => const Text('🔄 Connecting'),
      ConnectionStatus.online => const Text('🟢 Online'),
      ConnectionStatus.error => const Text('🔴 Error'),
    };

    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 24, 16, 12),
      child: Row(
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const Text(
                  'AI Chat',
                  style: TextStyle(fontSize: 20, fontWeight: FontWeight.bold),
                ),
                const SizedBox(height: 4),
                indicator,
              ],
            ),
          ),
          IconButton(
            tooltip: 'Refresh',
            onPressed: onRefresh,
            icon: const Icon(Icons.refresh),
          ),
        ],
      ),
    );
  }
}

class _EmptyPlaceholder extends StatelessWidget {
  const _EmptyPlaceholder();

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          const Icon(Icons.hourglass_empty, size: 48),
          const SizedBox(height: 8),
          const Text('No conversations yet.'),
          const SizedBox(height: 8),
          Text(
            'Tap “+ Start New” to begin a chat.',
            style: Theme.of(context).textTheme.bodySmall,
          ),
        ],
      ),
    );
  }
}

List<_ConversationItem> _items(AppState state) {
  if (state.searchQuery.isNotEmpty && state.searchResults.isNotEmpty) {
    return state.searchResults
        .map<_ConversationItem>(_ConversationItem.searchResult)
        .toList();
  }
  return state.conversations
      .map<_ConversationItem>(_ConversationItem.summary)
      .toList();
}

sealed class _ConversationItem {
  const _ConversationItem();

  T map<T>({
    required T Function(ConversationSummary summary) summary,
    required T Function(SearchResult result) searchResult,
  });

  factory _ConversationItem.summary(ConversationSummary summary) =
      _SummaryItem;
  factory _ConversationItem.searchResult(SearchResult result) =
      _SearchItem;
}

class _SummaryItem extends _ConversationItem {
  const _SummaryItem(this.summary);
  final ConversationSummary summary;

  @override
  T map<T>({
    required T Function(ConversationSummary summary) summary,
    required T Function(SearchResult result) searchResult,
  }) =>
      summary(this.summary);
}

class _SearchItem extends _ConversationItem {
  const _SearchItem(this.result);
  final SearchResult result;

  @override
  T map<T>({
    required T Function(ConversationSummary summary) summary,
    required T Function(SearchResult result) searchResult,
  }) =>
      searchResult(result);
}

