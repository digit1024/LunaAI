import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../application/app_controller.dart';
import '../../data/ws/ws_dto.dart';

/// Shows a dialog to edit a memory (content, category, importance).
Future<void> showEditMemoryDialog({
  required BuildContext context,
  required WidgetRef ref,
  required MemoryView memory,
}) async {
  final contentController = TextEditingController(text: memory.content);
  final categoryController =
      TextEditingController(text: memory.category ?? '');
  final importanceController =
      TextEditingController(text: memory.importance.toString());
  final formKey = GlobalKey<FormState>();

  await showDialog<void>(
    context: context,
    builder: (dialogContext) {
      return AlertDialog(
        title: const Text('Edit memory'),
        content: SingleChildScrollView(
          child: Form(
            key: formKey,
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                TextFormField(
                  controller: contentController,
                  autofocus: true,
                  maxLines: 5,
                  decoration: const InputDecoration(
                    labelText: 'Content',
                    alignLabelWithHint: true,
                  ),
                  validator: (value) {
                    if (value == null || value.trim().isEmpty) {
                      return 'Content cannot be empty';
                    }
                    return null;
                  },
                ),
                const SizedBox(height: 12),
                TextFormField(
                  controller: categoryController,
                  decoration: const InputDecoration(
                    labelText: 'Category (optional)',
                  ),
                ),
                const SizedBox(height: 12),
                TextFormField(
                  controller: importanceController,
                  keyboardType: TextInputType.number,
                  decoration: const InputDecoration(
                    labelText: 'Importance (1–10)',
                  ),
                  validator: (value) {
                    final parsed = int.tryParse(value?.trim() ?? '');
                    if (parsed == null || parsed < 1 || parsed > 10) {
                      return 'Enter a number from 1 to 10';
                    }
                    return null;
                  },
                ),
              ],
            ),
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () {
              if (formKey.currentState?.validate() ?? false) {
                final category = categoryController.text.trim();
                Navigator.of(dialogContext).pop();
                ref.read(appControllerProvider.notifier).updateMemory(
                      id: memory.id,
                      content: contentController.text.trim(),
                      category: category.isEmpty ? '' : category,
                      importance:
                          int.parse(importanceController.text.trim()),
                    );
              }
            },
            child: const Text('Save'),
          ),
        ],
      );
    },
  );

  contentController.dispose();
  categoryController.dispose();
  importanceController.dispose();
}
