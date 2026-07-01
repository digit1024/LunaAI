import 'package:flutter/material.dart';
import 'package:flutter_slidable/flutter_slidable.dart';

/// Swipe right reveals [startActions]; swipe left reveals [endActions].
class SwipeListTile extends StatelessWidget {
  const SwipeListTile({
    super.key,
    required this.slidableKey,
    required this.child,
    this.startActions = const [],
    this.endActions = const [],
    this.dismissEndOnFullSwipe = false,
    this.onDismissEnd,
  });

  final Key slidableKey;
  final Widget child;
  final List<SwipeAction> startActions;
  final List<SwipeAction> endActions;
  final bool dismissEndOnFullSwipe;
  final VoidCallback? onDismissEnd;

  @override
  Widget build(BuildContext context) {
    if (startActions.isEmpty && endActions.isEmpty) {
      return child;
    }

    return Slidable(
      key: slidableKey,
      startActionPane: startActions.isEmpty
          ? null
          : ActionPane(
              motion: const DrawerMotion(),
              extentRatio: _extentRatio(startActions.length),
              children: startActions.map(_buildAction).toList(),
            ),
      endActionPane: endActions.isEmpty
          ? null
          : ActionPane(
              motion: const DrawerMotion(),
              extentRatio: _extentRatio(endActions.length),
              dismissible: dismissEndOnFullSwipe && onDismissEnd != null
                  ? DismissiblePane(onDismissed: onDismissEnd!)
                  : null,
              children: endActions.map(_buildAction).toList(),
            ),
      child: child,
    );
  }

  double _extentRatio(int actionCount) {
    return (0.22 * actionCount).clamp(0.22, 0.55);
  }

  Widget _buildAction(SwipeAction action) {
    return SlidableAction(
      onPressed: (_) => action.onPressed(),
      backgroundColor: action.backgroundColor,
      foregroundColor: action.foregroundColor,
      icon: action.icon,
      label: action.label,
      autoClose: action.autoClose,
    );
  }
}

class SwipeAction {
  const SwipeAction({
    required this.icon,
    required this.label,
    required this.onPressed,
    required this.backgroundColor,
    this.foregroundColor = Colors.white,
    this.autoClose = true,
  });

  final IconData icon;
  final String label;
  final VoidCallback onPressed;
  final Color backgroundColor;
  final Color foregroundColor;
  final bool autoClose;
}
