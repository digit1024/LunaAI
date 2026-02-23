import 'dart:math' as math;
import 'package:flutter/material.dart';

import '../../application/app_state.dart';

// ── Config per state (background animation only) ───────────────────────────
class _StateConfig {
  final List<Color> colors;
  final double rippleSpeed; // animation duration multiplier

  const _StateConfig({
    required this.colors,
    required this.rippleSpeed,
  });
}

const _configs = {
  DialogModeState.listening: _StateConfig(
    colors: [
      Color(0xFF0D47A1),
      Color(0xFF00BCD4),
      Color(0xFF1A237E),
    ],
    rippleSpeed: 1.8, // slow steady pulse
  ),
  DialogModeState.processing: _StateConfig(
    colors: [
      Color(0xFF4A148C),
      Color(0xFF7B1FA2),
      Color(0xFF1A0533),
    ],
    rippleSpeed: 0.9, // fast, restless
  ),
  DialogModeState.speaking: _StateConfig(
    colors: [
      Color(0xFF1B5E20),
      Color(0xFF00E676),
      Color(0xFF003300),
    ],
    rippleSpeed: 1.3, // medium rhythm
  ),
};

// Single place to tune sound wave opacity (0.0 = invisible, 1.0 = full)
const _soundWaveAlphaMin = 0.05;
const _soundWaveAlphaMax = 0.1;

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
  // Breathing animation for listening (STT) — used on center icon
  late AnimationController _breatheController;
  late Animation<double> _breatheAnimation;

  // Pulse animation for speaking (TTS) — used on center icon
  late AnimationController _pulseController;
  late Animation<double> _pulseAnimation;

  // Bouncing dots animation for processing
  late AnimationController _dotsController;

  // Background-only: gradient breath + ripple rings (container-level, no impact on taps)
  late AnimationController _bgPulseCtrl;
  late AnimationController _bgRippleCtrl;
  // Wobble for processing icon only (sine left/right)
  late AnimationController _wobbleCtrl;
  // Sound wave bars (speaking only)
  late AnimationController _soundWaveCtrl;

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

    // Background: gradient pulse + ripple (duration depends on state)
    _createBackgroundControllers(widget.state);

    // Wobble for processing icon (sine wave left/right); started only when state == processing
    _wobbleCtrl = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 600),
    );

    // Sound waves (speaking only); started in _startAnimationForState
    _soundWaveCtrl = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 1200),
    );

    _startAnimationForState(widget.state);
  }

  void _createBackgroundControllers(DialogModeState state) {
    final cfg = _configs[state]!;
    final speed = cfg.rippleSpeed;
    _bgPulseCtrl = AnimationController(
      vsync: this,
      duration: Duration(milliseconds: (2000 / speed).round()),
    )..repeat(reverse: true);
    _bgRippleCtrl = AnimationController(
      vsync: this,
      duration: Duration(milliseconds: (2500 / speed).round()),
    )..repeat();
  }

  @override
  void didUpdateWidget(VoiceModeOverlay oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.state != widget.state) {
      _bgPulseCtrl.dispose();
      _bgRippleCtrl.dispose();
      _createBackgroundControllers(widget.state);
      _stopAllAnimations();
      _startAnimationForState(widget.state);
    }
  }

  void _stopAllAnimations() {
    _breatheController.stop();
    _pulseController.stop();
    _dotsController.stop();
    _wobbleCtrl.stop();
    _soundWaveCtrl.stop();
  }

  void _startAnimationForState(DialogModeState state) {
    switch (state) {
      case DialogModeState.listening:
        _breatheController.repeat(reverse: true);
        break;
      case DialogModeState.processing:
        _dotsController.repeat();
        _wobbleCtrl.repeat(reverse: true);
        break;
      case DialogModeState.speaking:
        _pulseController.repeat(reverse: true);
        _soundWaveCtrl.repeat();
        break;
    }
  }

  @override
  void dispose() {
    _breatheController.dispose();
    _pulseController.dispose();
    _dotsController.dispose();
    _bgPulseCtrl.dispose();
    _bgRippleCtrl.dispose();
    _wobbleCtrl.dispose();
    _soundWaveCtrl.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final cfg = _configs[widget.state]!;

    return Material(
      color: Colors.transparent,
      child: Stack(
        fit: StackFit.expand,
        children: [
          // ── Animated background (isolated layer to limit repaint scope and buffer pressure)
          RepaintBoundary(
            child: AnimatedSwitcher(
              duration: const Duration(milliseconds: 600),
              child: Stack(
                key: ValueKey(widget.state),
                fit: StackFit.expand,
                children: [
                  // Gradient breath
                  AnimatedBuilder(
                    animation: _bgPulseCtrl,
                    builder: (_, __) {
                      final t = _bgPulseCtrl.value;
                      return Container(
                        decoration: BoxDecoration(
                          gradient: RadialGradient(
                            center: Alignment.center,
                            radius: 0.8 + t * 0.4,
                            colors: [
                              cfg.colors[1].withValues(alpha: 0.25 + t * 0.1),
                              cfg.colors[0].withValues(alpha: 0.5),
                              cfg.colors[2],
                            ],
                          ),
                        ),
                      );
                    },
                  ),
                  // Ripple rings
                  AnimatedBuilder(
                    animation: _bgRippleCtrl,
                    builder: (_, __) => CustomPaint(
                      painter: _RipplePainter(
                        progress: _bgRippleCtrl.value,
                        color: cfg.colors[1],
                        rings: 3,
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
          // Dark overlay so content stays readable
          Container(color: Colors.black.withValues(alpha: 0.75)),
          // Sound waves (speaking only) — drawn on top of overlay so they’re visible with soft opacity
          if (widget.state == DialogModeState.speaking)
            Positioned.fill(
              child: RepaintBoundary(
                child: IgnorePointer(
                  child: AnimatedBuilder(
                    animation: _soundWaveCtrl,
                  builder: (_, __) => CustomPaint(
                    painter: _SoundWavePainter(
                      progress: _soundWaveCtrl.value,
                      color: cfg.colors[1],
                      alphaMin: _soundWaveAlphaMin,
                      alphaMax: _soundWaveAlphaMax,
                    ),
                  ),
                ),
              ),
            ),
          ),
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
          // Central icon with tap to stop TTS (isolated so icon animation doesn't repaint full overlay)
          RepaintBoundary(
            child: Center(
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

  /// ⋯ Processing: Three bouncing dots + left/right wobble (sine)
  Widget _buildProcessingIcon() {
    return AnimatedBuilder(
      animation: _wobbleCtrl,
      builder: (context, _) {
        final wobble = math.sin(_wobbleCtrl.value * math.pi) * 6.0;
        return Transform.translate(
          offset: Offset(wobble, 0),
          child: Container(
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
                      final delay = index * 0.2;
                      final progress = (_dotsController.value + delay) % 1.0;
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
          ),
        );
      },
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

// ── Ripple rings (background only) ─────────────────────────────────────────
class _RipplePainter extends CustomPainter {
  final double progress;
  final Color color;
  final int rings;

  _RipplePainter({
    required this.progress,
    required this.color,
    required this.rings,
  });

  @override
  void paint(Canvas canvas, Size size) {
    final center = Offset(size.width / 2, size.height / 2);
    final maxRadius = size.width * 0.55;

    for (int i = 0; i < rings; i++) {
      final t = (progress + i / rings) % 1.0;
      final radius = maxRadius * t;
      final opacity = (1.0 - t) * 0.35;

      canvas.drawCircle(
        center,
        radius,
        Paint()
          ..color = color.withValues(alpha: opacity)
          ..style = PaintingStyle.stroke
          ..strokeWidth = 2.0,
      );
    }
  }

  @override
  bool shouldRepaint(_RipplePainter old) =>
      old.progress != progress || old.color != color;
}

// ── Sound wave bars (speaking only; part of container background) ─────────
class _SoundWavePainter extends CustomPainter {
  final double progress;
  final Color color;
  final double alphaMin;
  final double alphaMax;

  _SoundWavePainter({
    required this.progress,
    required this.color,
    required this.alphaMin,
    required this.alphaMax,
  });

  static const int _barCount = 11;
  // Target ~80% of screen; bar/gap ratio kept proportional
  static const double _fillFraction = 0.8;

  @override
  void paint(Canvas canvas, Size size) {
    final contentWidth = size.width * _fillFraction;
    final barWidth = contentWidth / (_barCount + (_barCount - 1) * (4 / 5));
    final gap = barWidth * 4 / 5;
    final totalWidth = _barCount * barWidth + (_barCount - 1) * gap;
    final baseHeight = size.height * 0.02;
    final amplitude = size.height * 0.18;

    final centerX = size.width / 2;
    final centerY = size.height / 2;
    var x = centerX - totalWidth / 2 + barWidth / 2;
    final radius = (barWidth * 0.4).clamp(1.0, 4.0);

    for (var i = 0; i < _barCount; i++) {
      final phase = i * 0.6;
      final t = (progress * 2 * math.pi + phase) % (2 * math.pi);
      final h = baseHeight + math.sin(t) * amplitude;
      final top = centerY - h / 2;
      final barRect = RRect.fromRectAndRadius(
        Rect.fromLTWH(x - barWidth / 2, top, barWidth, h),
        Radius.circular(radius),
      );
      final alpha = alphaMin + (alphaMax - alphaMin) * (1 + math.sin(t)) / 2;
      if (alpha <= 0) continue; // 0.0 = fully invisible, skip draw
      final alphaInt = (alpha * 255).round().clamp(0, 255);
      canvas.drawRRect(
        barRect,
        Paint()
          ..color = color.withAlpha(alphaInt)
          ..style = PaintingStyle.fill,
      );
      x += barWidth + gap;
    }
  }

  @override
  bool shouldRepaint(_SoundWavePainter old) =>
      old.progress != progress ||
      old.alphaMin != alphaMin ||
      old.alphaMax != alphaMax;
}
