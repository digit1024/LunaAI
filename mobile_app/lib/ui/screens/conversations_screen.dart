import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_slidable/flutter_slidable.dart';

import '../../application/app_controller.dart';
import '../../application/app_state.dart';
import '../../core/config/server_config.dart';
import '../../data/ws/ws_dto.dart';
import '../../voice/voice_preferences.dart';
import '../widgets/conversation_card.dart';
import 'settings_screen.dart';
import '../widgets/rename_conversation_dialog.dart';

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
    final config = ref.watch(serverConfigProvider);

    ref.listen<String?>(
      appControllerProvider.select((s) => s.infoMessage),
      (_, infoMessage) {
        if (infoMessage != null && infoMessage.isNotEmpty) {
          ScaffoldMessenger.of(context).showSnackBar(
            SnackBar(content: Text(infoMessage)),
          );
          controller.clearInfoMessage();
        }
      },
    );

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

    return Scaffold(
      drawer: _ChatDrawer(
        profile: state.currentProfile.isNotEmpty ? state.currentProfile : config.profile,
        availableProfiles: state.availableProfiles,
        showInternal: state.showInternal,
        onToggleShowInternal: () {
          setState(() {
            _currentOffset = 0;
            _hasMore = true;
            _isLoadingMore = false;
            _previousConversationCount = 0;
          });
          controller.toggleShowInternal();
        },
        onProfileChanged: (newProfile) {
          controller.changeProfile(newProfile);
        },
        onStartNew: controller.startNewConversation,
        onHistory: controller.openConversations,
        onMemories: controller.openMemories,
        onMcpServers: controller.openMcpServers,
        onSettings: () {
          Navigator.of(context).push(
            MaterialPageRoute<void>(
              builder: (_) => const SettingsScreen(),
            ),
          );
        },
      ),
      body: Column(
        children: [
          _Header(
            status: state.connection,
            onMemories: controller.openMemories,
            onMcpServers: controller.openMcpServers,
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
              onChanged: controller.search,
            ),
          ),
          Expanded(
            child: _items(state).isEmpty
                ? (_isSearchActive(state)
                    ? const _SearchEmptyPlaceholder()
                    : const _EmptyPlaceholder())
                : SlidableAutoCloseBehavior(
                    child: ListView.separated(
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
                          return ConversationCard(
                            summary: summary,
                            isSelected: selected,
                            onTap: () => controller.selectConversation(summary.id),
                            onEdit: () => showRenameConversationDialog(
                              context: context,
                              ref: ref,
                              conversationId: summary.id,
                              currentTitle: summary.title,
                            ),
                            onDelete: () => _confirmDeleteConversation(
                              context,
                              controller,
                              summary,
                            ),
                            onToggleTransient: () => controller.setConversationInternal(
                              summary.id,
                              !summary.internal,
                            ),
                          );
                        },
                        searchResult: (result) {
                          final title = result.conversationTitle.isNotEmpty
                              ? result.conversationTitle
                              : 'Untitled';
                          return ListTile(
                            leading: const Icon(Icons.history),
                            title: Text(title),
                            subtitle: Text(
                              result.snippet,
                              maxLines: 2,
                              overflow: TextOverflow.ellipsis,
                            ),
                            onTap: () => controller.selectConversation(
                              result.conversationId,
                            ),
                          );
                        },
                      );
                    },
                  ),
                  ),
          ),
        ],
      ),
      floatingActionButton: FloatingActionButton(
        onPressed: () => controller.startNewConversation(),
        tooltip: 'New Chat',
        child: const Icon(Icons.add),
      ),
    );
  }

  Future<void> _confirmDeleteConversation(
    BuildContext context,
    AppController controller,
    ConversationSummary summary,
  ) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Delete conversation?'),
        content: Text(
          summary.title.isNotEmpty ? summary.title : 'Untitled',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            style: FilledButton.styleFrom(
              backgroundColor: Theme.of(context).colorScheme.error,
            ),
            child: const Text('Delete'),
          ),
        ],
      ),
    );
    if (confirmed == true) {
      controller.deleteConversation(summary.id);
    }
  }
}

class _Header extends StatelessWidget {
  const _Header({
    required this.status,
    required this.onRefresh,
    required this.onMemories,
    required this.onMcpServers,
  });

  final ConnectionStatus status;
  final VoidCallback onRefresh;
  final VoidCallback onMemories;
  final VoidCallback onMcpServers;

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
          Builder(
            builder: (context) => IconButton(
              icon: const Icon(Icons.menu),
              onPressed: () => Scaffold.of(context).openDrawer(),
              tooltip: 'Menu',
            ),
          ),
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
          PopupMenuButton<String>(
            onSelected: (value) {
              if (value == 'memories') onMemories();
              if (value == 'mcp') onMcpServers();
            },
            itemBuilder: (context) => [
              const PopupMenuItem(
                value: 'memories',
                child: ListTile(
                  leading: Icon(Icons.psychology_outlined),
                  title: Text('Memories'),
                  contentPadding: EdgeInsets.zero,
                ),
              ),
              const PopupMenuItem(
                value: 'mcp',
                child: ListTile(
                  leading: Icon(Icons.hub_outlined),
                  title: Text('MCP Servers'),
                  contentPadding: EdgeInsets.zero,
                ),
              ),
            ],
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

class _SearchEmptyPlaceholder extends StatelessWidget {
  const _SearchEmptyPlaceholder();

  @override
  Widget build(BuildContext context) {
    return const Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(Icons.search_off, size: 48),
          SizedBox(height: 8),
          Text('No matching conversations'),
          SizedBox(height: 8),
          Text('Try a different search term'),
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

bool _isSearchActive(AppState state) => state.searchQuery.isNotEmpty;

List<_ConversationItem> _items(AppState state) {
  if (_isSearchActive(state)) {
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

class _ChatDrawer extends ConsumerWidget {
  const _ChatDrawer({
    required this.profile,
    required this.availableProfiles,
    required this.showInternal,
    required this.onToggleShowInternal,
    required this.onProfileChanged,
    required this.onStartNew,
    required this.onHistory,
    required this.onMemories,
    required this.onMcpServers,
    required this.onSettings,
  });

  final String profile;
  final List<String> availableProfiles;
  final bool showInternal;
  final VoidCallback onToggleShowInternal;
  final Function(String) onProfileChanged;
  final VoidCallback onStartNew;
  final VoidCallback onHistory;
  final VoidCallback onMemories;
  final VoidCallback onMcpServers;
  final VoidCallback onSettings;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final voicePrefs = ref.watch(voicePreferencesProvider);
    final voiceNotifier = ref.read(voicePreferencesProvider.notifier);
    return Drawer(
      child: ListView(
        padding: EdgeInsets.zero,
        children: [
          DrawerHeader(
            decoration: BoxDecoration(
              color: Theme.of(context).colorScheme.primaryContainer,
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                Text(
                  'Luna AI',
                  style: Theme.of(context).textTheme.headlineSmall?.copyWith(
                        color:
                            Theme.of(context).colorScheme.onPrimaryContainer,
                      ),
                ),
              ],
            ),
          ),
          ListTile(
            leading: const Icon(Icons.add),
            title: const Text('New Chat'),
            onTap: () {
              Navigator.pop(context);
              onStartNew();
            },
          ),
          SwitchListTile(
            secondary: const Icon(Icons.volume_up),
            title: const Text('Text-to-Speech'),
            subtitle: const Text('Read assistant replies aloud'),
            value: voicePrefs.ttsEnabled,
            onChanged: (value) {
              voiceNotifier.setTtsEnabled(value);
            },
          ),
          const Divider(),
          if (availableProfiles.isNotEmpty)
            ExpansionTile(
              leading: const Icon(Icons.person),
              title: const Text('Profile'),
              subtitle: Text(profile),
              children: availableProfiles.map((p) {
                return ListTile(
                  title: Text(p),
                  selected: p == profile,
                  onTap: () {
                    Navigator.pop(context);
                    onProfileChanged(p);
                  },
                );
              }).toList(),
            ),
          ListTile(
            leading: const Icon(Icons.history),
            title: const Text('History'),
            onTap: () {
              Navigator.pop(context);
              onHistory();
            },
          ),
          SwitchListTile(
            secondary: const Icon(Icons.visibility_off_outlined),
            title: const Text('Show internal'),
            subtitle: const Text('Include internal conversations in history'),
            value: showInternal,
            onChanged: (_) {
              Navigator.pop(context);
              onToggleShowInternal();
            },
          ),
          ListTile(
            leading: const Icon(Icons.psychology_outlined),
            title: const Text('Memories'),
            onTap: () {
              Navigator.pop(context);
              onMemories();
            },
          ),
          ListTile(
            leading: const Icon(Icons.hub_outlined),
            title: const Text('MCP Servers'),
            onTap: () {
              Navigator.pop(context);
              onMcpServers();
            },
          ),
          const Divider(),
          ListTile(
            leading: const Icon(Icons.settings),
            title: const Text('Settings'),
            onTap: () {
              Navigator.pop(context);
              onSettings();
            },
          ),
        ],
      ),
    );
  }
}
