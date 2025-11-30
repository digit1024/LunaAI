import 'package:flutter/material.dart';
import 'dart:math' as math;
import '../../application/app_state.dart';

class VoiceModeOverlay extends StatefulWidget {
  const VoiceModeOverlay({
    super.key,
    required this.state,
    required this.onClose,
    required this.onStopTts,
  });

  final DialogModeState state;
  final VoidCallback onClose;
  final VoidCallback onStopTts;

  @override
  State<VoiceModeOverlay> createState() => _VoiceModeOverlayState();
}

class _VoiceModeOverlayState extends State<VoiceModeOverlay>
    with TickerProviderStateMixin {
  // Breathing animation for listening (STT)
  late AnimationController _breatheController;
  late Animation<double> _breatheAnimation;

  // Pulse animation for speaking (TTS)
  late AnimationController _pulseController;
  late Animation<double> _pulseAnimation;

  // Bouncing dots animation for processing
  late AnimationController _dotsController;

  @override
  void initState() {
    super.initState();

    // Breathing: smooth scale 1.0 → 1.08 → 1.0 (subtle)
    _breatheController = AnimationController(
      duration: const Duration(milliseconds: 1500),
      vsync: this,
    );
    _breatheAnimation = Tween<double>(begin: 1.0, end: 1.08).animate(
      CurvedAnimation(parent: _breatheController, curve: Curves.easeInOut),
    );

    // Pulse: scale 1.0 → 1.15 → 1.0 (more noticeable)
    _pulseController = AnimationController(
      duration: const Duration(milliseconds: 800),
      vsync: this,
    );
    _pulseAnimation = Tween<double>(begin: 1.0, end: 1.15).animate(
      CurvedAnimation(parent: _pulseController, curve: Curves.easeInOut),
    );

    // Dots bouncing
    _dotsController = AnimationController(
      duration: const Duration(milliseconds: 1200),
      vsync: this,
    );

    _startAnimationForState(widget.state);
  }

  @override
  void didUpdateWidget(VoiceModeOverlay oldWidget) {
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
          // Close button in top-right
          Positioned(
            top: 48,
            right: 16,
            child: IconButton(
              icon: const Icon(Icons.close, size: 32),
              color: Colors.white70,
              onPressed: widget.onClose,
            ),
          ),
          // Central icon with tap to stop TTS
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
                const SizedBox(height: 24),
                AnimatedSwitcher(
                  duration: const Duration(milliseconds: 200),
                  child: Text(
                    _getHintText(widget.state),
                    key: ValueKey('hint_${widget.state}'),
                    style: const TextStyle(
                      color: Colors.white70,
                      fontSize: 16,
                      fontWeight: FontWeight.w300,
                      letterSpacing: 1.2,
                    ),
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

  /// 🎤 Listening: Mic with breathing circle
  Widget _buildListeningIcon() {
    return AnimatedBuilder(
      animation: _breatheAnimation,
      builder: (context, child) {
        return Transform.scale(
          scale: _breatheAnimation.value,
          child: Container(
            key: const ValueKey('listening'),
            width: 160,
            height: 160,
            decoration: BoxDecoration(
              shape: BoxShape.circle,
              color: Colors.red.withValues(alpha: 0.15),
              border: Border.all(
                color: Colors.red.withValues(alpha: 0.6),
                width: 3,
              ),
              boxShadow: [
                BoxShadow(
                  color: Colors.red.withValues(alpha: 0.3),
                  blurRadius: 20,
                  spreadRadius: 5,
                ),
              ],
            ),
            child: const Icon(
              Icons.mic,
              color: Colors.red,
              size: 80,
            ),
          ),
        );
      },
    );
  }

  /// ⋯ Processing: Three bouncing dots
  Widget _buildProcessingIcon() {
    return Container(
      key: const ValueKey('processing'),
      width: 160,
      height: 160,
      decoration: BoxDecoration(
        shape: BoxShape.circle,
        color: Colors.blue.withValues(alpha: 0.15),
        border: Border.all(
          color: Colors.blue.withValues(alpha: 0.6),
          width: 3,
        ),
      ),
      child: Center(
        child: AnimatedBuilder(
          animation: _dotsController,
          builder: (context, child) {
            return Row(
              mainAxisSize: MainAxisSize.min,
              children: List.generate(3, (index) {
                // Stagger the animation for each dot
                final delay = index * 0.2;
                final progress = (_dotsController.value + delay) % 1.0;
                // Bounce curve: sin wave
                final bounce = math.sin(progress * math.pi);
                return Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 6),
                  child: Transform.translate(
                    offset: Offset(0, -bounce * 20),
                    child: Container(
                      width: 18,
                      height: 18,
                      decoration: BoxDecoration(
                        shape: BoxShape.circle,
                        color: Colors.blue.withValues(alpha: 0.8 + bounce * 0.2),
                        boxShadow: [
                          BoxShadow(
                            color: Colors.blue.withValues(alpha: bounce * 0.5),
                            blurRadius: 8,
                            spreadRadius: 2,
                          ),
                        ],
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

  /// 🔊 Speaking: Pulsating speaker icon
  Widget _buildSpeakingIcon() {
    return AnimatedBuilder(
      animation: _pulseAnimation,
      builder: (context, child) {
        final scale = _pulseAnimation.value;
        return Container(
          key: const ValueKey('speaking'),
          width: 160,
          height: 160,
          decoration: BoxDecoration(
            shape: BoxShape.circle,
            color: Colors.green.withValues(alpha: 0.15),
            border: Border.all(
              color: Colors.green.withValues(alpha: 0.6),
              width: 3,
            ),
            boxShadow: [
              BoxShadow(
                color: Colors.green.withValues(alpha: (scale - 1.0) * 2),
                blurRadius: 25,
                spreadRadius: 8,
              ),
            ],
          ),
          child: Transform.scale(
            scale: scale,
            child: const Icon(
              Icons.volume_up,
              color: Colors.green,
              size: 80,
            ),
          ),
        );
      },
    );
  }
}
