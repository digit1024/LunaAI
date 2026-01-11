import 'package:flutter/material.dart';
import 'dart:math' as math;
import 'package:luna_mobile/application/app_state.dart';

class WearVoiceOverlay extends StatefulWidget {
  const WearVoiceOverlay({
    super.key,
    required this.state,
    required this.onClose,
    required this.onStopTts,
  });

  final DialogModeState state;
  final VoidCallback onClose;
  final VoidCallback onStopTts;

  @override
  State<WearVoiceOverlay> createState() => _WearVoiceOverlayState();
}

class _WearVoiceOverlayState extends State<WearVoiceOverlay>
    with TickerProviderStateMixin {
  late AnimationController _breatheController;
  late Animation<double> _breatheAnimation;
  late AnimationController _pulseController;
  late Animation<double> _pulseAnimation;
  late AnimationController _dotsController;

  @override
  void initState() {
    super.initState();
    _breatheController = AnimationController(
      duration: const Duration(milliseconds: 1500),
      vsync: this,
    );
    _breatheAnimation = Tween<double>(begin: 1.0, end: 1.08).animate(
      CurvedAnimation(parent: _breatheController, curve: Curves.easeInOut),
    );
    _pulseController = AnimationController(
      duration: const Duration(milliseconds: 800),
      vsync: this,
    );
    _pulseAnimation = Tween<double>(begin: 1.0, end: 1.15).animate(
      CurvedAnimation(parent: _pulseController, curve: Curves.easeInOut),
    );
    _dotsController = AnimationController(
      duration: const Duration(milliseconds: 1200),
      vsync: this,
    );
    _startAnimationForState(widget.state);
  }

  @override
  void didUpdateWidget(WearVoiceOverlay oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.state != widget.state) {
      _stopAllAnimations();
      _startAnimationForState(widget.state);
    }
  }

  void _stopAllAnimations() {
    _breatheController.stop();
    _pulseController.stop();
    _dotsController.stop();
  }

  void _startAnimationForState(DialogModeState state) {
    switch (state) {
      case DialogModeState.listening:
        _breatheController.repeat(reverse: true);
        break;
      case DialogModeState.processing:
        _dotsController.repeat();
        break;
      case DialogModeState.speaking:
        _pulseController.repeat(reverse: true);
        break;
    }
  }

  @override
  void dispose() {
    _breatheController.dispose();
    _pulseController.dispose();
    _dotsController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Material(
      color: Colors.black.withValues(alpha: 0.85),
      child: Stack(
        children: [
          Positioned(
            top: 8,
            right: 8,
            child: IconButton(
              icon: const Icon(Icons.close, size: 20),
              color: Colors.white70,
              onPressed: widget.onClose,
            ),
          ),
          Center(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                GestureDetector(
                  onTap: widget.state == DialogModeState.speaking
                      ? widget.onStopTts
                      : null,
                  child: AnimatedSwitcher(
                    duration: const Duration(milliseconds: 300),
                    child: _buildAnimatedIcon(widget.state),
                  ),
                ),
                const SizedBox(height: 12),
                Text(
                  _getHintText(widget.state),
                  style: const TextStyle(
                    color: Colors.white70,
                    fontSize: 12,
                    fontWeight: FontWeight.w300,
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }

  String _getHintText(DialogModeState state) {
    switch (state) {
      case DialogModeState.listening:
        return 'Listening...';
      case DialogModeState.processing:
        return 'Thinking...';
      case DialogModeState.speaking:
        return 'Tap to stop';
    }
  }

  Widget _buildAnimatedIcon(DialogModeState state) {
    switch (state) {
      case DialogModeState.listening:
        return _buildListeningIcon();
      case DialogModeState.processing:
        return _buildProcessingIcon();
      case DialogModeState.speaking:
        return _buildSpeakingIcon();
    }
  }

  Widget _buildListeningIcon() {
    return AnimatedBuilder(
      animation: _breatheAnimation,
      builder: (context, child) {
        return Transform.scale(
          scale: _breatheAnimation.value,
          child: Container(
            key: const ValueKey('listening'),
            width: 80,
            height: 80,
            decoration: BoxDecoration(
              shape: BoxShape.circle,
              color: Colors.red.withValues(alpha: 0.15),
              border: Border.all(
                color: Colors.red.withValues(alpha: 0.6),
                width: 2,
              ),
            ),
            child: const Icon(
              Icons.mic,
              color: Colors.red,
              size: 40,
            ),
          ),
        );
      },
    );
  }

  Widget _buildProcessingIcon() {
    return Container(
      key: const ValueKey('processing'),
      width: 80,
      height: 80,
      decoration: BoxDecoration(
        shape: BoxShape.circle,
        color: Colors.blue.withValues(alpha: 0.15),
        border: Border.all(
          color: Colors.blue.withValues(alpha: 0.6),
          width: 2,
        ),
      ),
      child: Center(
        child: AnimatedBuilder(
          animation: _dotsController,
          builder: (context, child) {
            return Row(
              mainAxisSize: MainAxisSize.min,
              children: List.generate(3, (index) {
                final delay = index * 0.2;
                final progress = (_dotsController.value + delay) % 1.0;
                final bounce = math.sin(progress * math.pi);
                return Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 3),
                  child: Transform.translate(
                    offset: Offset(0, -bounce * 10),
                    child: Container(
                      width: 8,
                      height: 8,
                      decoration: BoxDecoration(
                        shape: BoxShape.circle,
                        color: Colors.blue.withValues(alpha: 0.8 + bounce * 0.2),
                      ),
                    ),
                  ),
                );
              }),
            );
          },
        ),
      ),
    );
  }

  Widget _buildSpeakingIcon() {
    return AnimatedBuilder(
      animation: _pulseAnimation,
      builder: (context, child) {
        return Container(
          key: const ValueKey('speaking'),
          width: 80,
          height: 80,
          decoration: BoxDecoration(
            shape: BoxShape.circle,
            color: Colors.green.withValues(alpha: 0.15),
            border: Border.all(
              color: Colors.green.withValues(alpha: 0.6),
              width: 2,
            ),
          ),
          child: Transform.scale(
            scale: _pulseAnimation.value,
            child: const Icon(
              Icons.volume_up,
              color: Colors.green,
              size: 40,
            ),
          ),
        );
      },
    );
  }
}

