import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../application/app_controller.dart';
import '../../application/app_state.dart';
import '../../data/ws/ws_dto.dart';
import '../widgets/bottom_nav.dart';
import '../widgets/conversation_card.dart';

class ConversationsScreen extends ConsumerStatefulWidget {
  const ConversationsScreen({super.key});

  @override
  ConsumerState<ConversationsScreen> createState() => _ConversationsScreenState();
}

class _ConversationsScreenState extends ConsumerState<ConversationsScreen> {
  final ScrollController _scrollController = ScrollController();
  bool _isLoadingMore = false;
  bool _hasMore = true;
  int _currentOffset = 0;
  int _previousConversationCount = 0;

  @override
  void initState() {
    super.initState();
    _scrollController.addListener(_onScroll);
    // Load initial conversations
    WidgetsBinding.instance.addPostFrameCallback((_) {
      ref.read(appControllerProvider.notifier).refreshConversations();
    });
  }

  @override
  void dispose() {
    _scrollController.dispose();
    super.dispose();
  }

  void _onScroll() {
    if (_scrollController.position.pixels >=
            _scrollController.position.maxScrollExtent * 0.8 &&
        !_isLoadingMore &&
        _hasMore) {
      _loadMore();
    }
  }

  void _loadMore() {
    if (_isLoadingMore || !_hasMore) return;
    setState(() {
      _isLoadingMore = true;
      _previousConversationCount = ref.read(appControllerProvider).conversations.length;
    });
    final nextOffset = _currentOffset + 10;
    ref.read(appControllerProvider.notifier).loadMoreConversations(nextOffset);
    // Reset loading state after a delay (will be updated when new data arrives)
    Future.delayed(const Duration(seconds: 1), () {
      if (mounted) {
        setState(() {
          _isLoadingMore = false;
        });
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    final state = ref.watch(appControllerProvider);
    final controller = ref.read(appControllerProvider.notifier);

    // Update offset and hasMore when conversations change
    final currentCount = state.conversations.length;
    
    // Check if conversations increased (pagination) or changed (refresh)
    if (currentCount > _previousConversationCount) {
      // Conversations increased - this could be pagination
      final addedCount = currentCount - _previousConversationCount;
      
      // If count exceeds current offset, it's pagination - update offset
      if (currentCount > _currentOffset) {
        _currentOffset = currentCount;
        // If we got fewer than 10 items, we've reached the end
        if (addedCount < 10) {
          _hasMore = false;
        }
      }
    } else if (currentCount != _previousConversationCount && _previousConversationCount > 0) {
      // Count changed but didn't increase - this was a refresh
      // Reset offset to current count
      _currentOffset = currentCount;
      // Re-enable hasMore if we have 10 or more items
      if (currentCount >= 10) {
        _hasMore = true;
      } else {
        _hasMore = false;
      }
    } else if (_currentOffset == 0 && currentCount < 10 && currentCount > 0) {
      // Initial load with fewer than 10 items
      _hasMore = false;
    }
    
    // Update previous count for next comparison
    _previousConversationCount = currentCount;

    return Column(
      children: [
        _Header(
          status: state.connection,
          onRefresh: () {
            setState(() {
              _currentOffset = 0;
              _hasMore = true;
              _isLoadingMore = false;
              _previousConversationCount = 0;
            });
            controller.refreshConversations();
          },
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
                  controller: _scrollController,
                  itemCount: _items(state).length + (_hasMore && !state.searchQuery.isNotEmpty ? 1 : 0),
                  separatorBuilder: (_, __) => const Divider(height: 0),
                  itemBuilder: (context, index) {
                    if (index >= _items(state).length) {
                      return const Center(
                        child: Padding(
                          padding: EdgeInsets.all(16.0),
                          child: CircularProgressIndicator(),
                        ),
                      );
                    }
                    final entry = _items(state)[index];
                    return entry.map(
                      summary: (summary) {
                        final selected =
                            state.activeConversation?.id == summary.id;
                        return Dismissible(
                          key: Key(summary.id),
                          direction: DismissDirection.endToStart,
                          background: Container(
                            color: Colors.red,
                            alignment: Alignment.centerRight,
                            padding: const EdgeInsets.only(right: 20),
                            child: const Icon(Icons.delete, color: Colors.white),
                          ),
                          onDismissed: (direction) {
                            controller.deleteConversation(summary.id);
                          },
                          child: ConversationCard(
                            summary: summary,
                            isSelected: selected,
                            onTap: () => controller.selectConversation(summary.id),
                          ),
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
          onSettings: controller.openSettings,
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

