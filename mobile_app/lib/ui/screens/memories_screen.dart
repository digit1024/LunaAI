import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:intl/intl.dart';

import '../../application/app_controller.dart';
import '../../data/ws/ws_dto.dart';
import '../widgets/edit_memory_dialog.dart';

class MemoriesScreen extends ConsumerWidget {
  const MemoriesScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final state = ref.watch(appControllerProvider);
    final controller = ref.read(appControllerProvider.notifier);
    final searching = state.memoriesSearch.trim().isNotEmpty;

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

    return Scaffold(
      appBar: AppBar(
        title: const Text('Memories'),
        leading: IconButton(
          icon: const Icon(Icons.arrow_back),
          onPressed: controller.openConversations,
        ),
        actions: [
          IconButton(
            tooltip: 'Refresh',
            icon: const Icon(Icons.refresh),
            onPressed: () => controller.refreshMemories(),
          ),
        ],
      ),
      body: Column(
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 8, 16, 8),
            child: TextField(
              decoration: const InputDecoration(
                prefixIcon: Icon(Icons.search),
                hintText: 'Search memories…',
              ),
              onChanged: controller.searchMemories,
            ),
          ),
          Expanded(
            child: state.memories.isEmpty
                ? _MemoriesEmptyState(searching: searching)
                : ListView.separated(
                    padding: const EdgeInsets.symmetric(vertical: 8),
                    itemCount: state.memories.length,
                    separatorBuilder: (_, __) => const Divider(height: 0),
                    itemBuilder: (context, index) {
                      final memory = state.memories[index];
                      return _MemoryTile(
                        memory: memory,
                        onEdit: () => showEditMemoryDialog(
                          context: context,
                          ref: ref,
                          memory: memory,
                        ),
                        onDelete: () => _confirmDelete(context, controller, memory),
                      );
                    },
                  ),
          ),
        ],
      ),
    );
  }

  Future<void> _confirmDelete(
    BuildContext context,
    AppController controller,
    MemoryView memory,
  ) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Delete memory?'),
        content: const Text(
          'This will permanently remove the memory and its vector index.',
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
      controller.deleteMemory(memory.id);
    }
  }
}

class _MemoriesEmptyState extends StatelessWidget {
  const _MemoriesEmptyState({required this.searching});

  final bool searching;

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(
            searching ? Icons.search_off : Icons.psychology_outlined,
            size: 48,
          ),
          const SizedBox(height: 8),
          Text(
            searching ? 'No matching memories' : 'No memories yet',
            style: Theme.of(context).textTheme.titleMedium,
          ),
          const SizedBox(height: 8),
          Text(
            searching
                ? 'Try a different search term'
                : 'Memories are stored when the assistant learns facts across conversations.',
            style: Theme.of(context).textTheme.bodySmall,
            textAlign: TextAlign.center,
          ),
        ],
      ),
    );
  }
}

class _MemoryTile extends StatelessWidget {
  const _MemoryTile({
    required this.memory,
    required this.onEdit,
    required this.onDelete,
  });

  final MemoryView memory;
  final VoidCallback onEdit;
  final VoidCallback onDelete;

  @override
  Widget build(BuildContext context) {
    final categoryPrefix = memory.category != null && memory.category!.isNotEmpty
        ? '[${memory.category}] '
        : '';
    final updated = DateTime.fromMillisecondsSinceEpoch(memory.updatedAt * 1000);
    final dateStr = DateFormat('yyyy-MM-dd HH:mm').format(updated);

    return ListTile(
      title: Text(
        '$categoryPrefix${memory.content}',
        maxLines: 3,
        overflow: TextOverflow.ellipsis,
      ),
      subtitle: Text('Importance ${memory.importance} · $dateStr'),
      trailing: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          IconButton(
            icon: const Icon(Icons.edit_outlined),
            tooltip: 'Edit',
            onPressed: onEdit,
          ),
          IconButton(
            icon: const Icon(Icons.delete_outline),
            tooltip: 'Delete',
            onPressed: onDelete,
          ),
        ],
      ),
    );
  }
}
