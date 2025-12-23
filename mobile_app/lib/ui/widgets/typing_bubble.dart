import 'dart:math' as math;
import 'package:flutter/material.dart';

class TypingBubble extends StatefulWidget {
  const TypingBubble({super.key});

  @override
  State<TypingBubble> createState() => _TypingBubbleState();
}

class _TypingBubbleState extends State<TypingBubble>
    with SingleTickerProviderStateMixin {
  late final AnimationController _waveController;

  @override
  void initState() {
    super.initState();
    _waveController = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 1200),
    )..repeat();
  }

  @override
  void dispose() {
    _waveController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final colorScheme = Theme.of(context).colorScheme;

    return Align(
      alignment: Alignment.centerLeft,
      child: Card(
        color: colorScheme.surfaceContainerHighest,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(18),
        ),
        child: SizedBox(
          width: 72, // Fixed width
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 14),
            child: Row(
              mainAxisAlignment: MainAxisAlignment.center,
              children: List.generate(3, (index) {
                return _BouncingDot(
                  controller: _waveController,
                  index: index,
                  colorScheme: colorScheme,
                );
              }),
            ),
          ),
        ),
      ),
    );
  }
}

class _BouncingDot extends StatelessWidget {
  const _BouncingDot({
    required this.controller,
    required this.index,
    required this.colorScheme,
  });

  final Animation<double> controller;
  final int index;
  final ColorScheme colorScheme;

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: controller,
      builder: (context, _) {
        // Staggered wave
        final phase = (controller.value + (index * 0.2)) % 1.0;
        final wave = math.sin(phase * math.pi * 2);
        
        // Bounce height
        final yOffset = wave * 4;
        
        // Scale pulse
        final scale = 0.7 + (0.3 * ((wave + 1) / 2));
        
        // Color shift
        final color = Color.lerp(
          colorScheme.primary,
          colorScheme.tertiary,
          (wave + 1) / 2,
        )!;

        return Padding(
          padding: const EdgeInsets.symmetric(horizontal: 3),
          child: Transform.translate(
            offset: Offset(0, yOffset),
            child: Transform.scale(
              scale: scale,
              child: Container(
                width: 7,
                height: 7,
                decoration: BoxDecoration(
                  shape: BoxShape.circle,
                  color: color,
                ),
              ),
            ),
          ),
        );
      },
    );
  }
}
