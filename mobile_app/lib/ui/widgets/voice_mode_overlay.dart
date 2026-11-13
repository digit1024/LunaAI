import 'package:flutter/material.dart';

import '../../application/app_state.dart';
import '../../voice/voice_models.dart';

class VoiceModeOverlay extends StatefulWidget {
  const VoiceModeOverlay({
    super.key,
    required this.state,
    required this.inputMode,
    required this.elapsedSeconds,
    required this.errorMessage,
    required this.onStartListening,
    required this.onStopListening,
    required this.onClose,
    required this.onRetry,
    required this.onOpenSettings,
  });

  final VoiceModeState state;
  final VoiceInputMode inputMode;
  final int elapsedSeconds;
  final String? errorMessage;
  final VoidCallback onStartListening;
  final VoidCallback onStopListening;
  final VoidCallback onClose;
  final VoidCallback onRetry;
  final VoidCallback onOpenSettings;

  @override
  State<VoiceModeOverlay> createState() => _VoiceModeOverlayState();
}

class _VoiceModeOverlayState extends State<VoiceModeOverlay>
    with SingleTickerProviderStateMixin {
  late final AnimationController _pulse;
  bool _pointerDown = false;

  @override
  void initState() {
    super.initState();
    _pulse = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 1400),
    )..repeat(reverse: true);
  }

  @override
  void didUpdateWidget(VoiceModeOverlay oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.inputMode == VoiceInputMode.hold &&
        widget.state == VoiceModeState.listening &&
        oldWidget.state != VoiceModeState.listening &&
        !_pointerDown) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted && widget.state == VoiceModeState.listening) {
          widget.onStopListening();
        }
      });
    }
  }

  @override
  void dispose() {
    _pulse.dispose();
    super.dispose();
  }

  bool get _canStart =>
      widget.state == VoiceModeState.idle ||
      widget.state == VoiceModeState.speaking ||
      widget.state == VoiceModeState.error;

  bool get _canStop => widget.state == VoiceModeState.listening;

  void _handleTap() {
    if (widget.inputMode != VoiceInputMode.tap) return;
    if (_canStart) {
      widget.onStartListening();
    } else if (_canStop) {
      widget.onStopListening();
    }
  }

  void _handlePointerDown(PointerDownEvent _) {
    _pointerDown = true;
    if (widget.inputMode == VoiceInputMode.hold && _canStart) {
      widget.onStartListening();
    }
  }

  void _handlePointerUp(PointerUpEvent _) {
    _pointerDown = false;
    if (widget.inputMode == VoiceInputMode.hold && _canStop) {
      widget.onStopListening();
    }
  }

  void _handlePointerCancel(PointerCancelEvent _) {
    _pointerDown = false;
    if (widget.inputMode == VoiceInputMode.hold && _canStop) {
      widget.onStopListening();
    }
  }

  @override
  Widget build(BuildContext context) {
    final colors = _colorsFor(widget.state);
    final hint = _hintText();
    final seconds = widget.elapsedSeconds;
    final timer = seconds > 0
        ? '${seconds ~/ 60}:${(seconds % 60).toString().padLeft(2, '0')}'
        : null;

    return Material(
      color: Colors.transparent,
      child: Stack(
        fit: StackFit.expand,
        children: [
          AnimatedContainer(
            duration: const Duration(milliseconds: 400),
            decoration: BoxDecoration(
              gradient: LinearGradient(
                begin: Alignment.topLeft,
                end: Alignment.bottomRight,
                colors: [
                  colors.$1.withValues(alpha: 0.92),
                  colors.$2.withValues(alpha: 0.96),
                  Colors.black,
                ],
              ),
            ),
          ),
          Positioned(
            top: 48,
            right: 16,
            child: IconButton(
              icon: const Icon(Icons.close, size: 32),
              color: Colors.white70,
              onPressed: widget.onClose,
            ),
          ),
          Center(
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 32),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  if (widget.state == VoiceModeState.error) ...[
                    _ErrorPanel(
                      message: widget.errorMessage ?? 'Voice mode failed',
                      onRetry: widget.onRetry,
                      onOpenSettings: widget.onOpenSettings,
                    ),
                  ] else ...[
                    Listener(
                      onPointerDown: _handlePointerDown,
                      onPointerUp: _handlePointerUp,
                      onPointerCancel: _handlePointerCancel,
                      child: GestureDetector(
                        onTap: _handleTap,
                        child: _buildCenterButton(colors.$3),
                      ),
                    ),
                    const SizedBox(height: 28),
                    AnimatedSwitcher(
                      duration: const Duration(milliseconds: 200),
                      child: Text(
                        hint,
                        key: ValueKey('hint_${widget.state}'),
                        textAlign: TextAlign.center,
                        style: const TextStyle(
                          color: Colors.white,
                          fontSize: 17,
                          fontWeight: FontWeight.w300,
                          letterSpacing: 0.6,
                        ),
                      ),
                    ),
                    if (timer != null) ...[
                      const SizedBox(height: 8),
                      Text(
                        timer,
                        style: const TextStyle(
                          color: Colors.white70,
                          fontSize: 14,
                          fontFeatures: [FontFeature.tabularFigures()],
                        ),
                      ),
                    ],
                  ],
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildCenterButton(Color accent) {
    final state = widget.state;
    if (state == VoiceModeState.transcribing ||
        state == VoiceModeState.thinking) {
      return SizedBox(
        width: 160,
        height: 160,
        child: Center(
          child: CircularProgressIndicator(color: accent, strokeWidth: 4),
        ),
      );
    }

    final listening = state == VoiceModeState.listening;
    final speaking = state == VoiceModeState.speaking;
    final icon = listening
        ? Icons.mic
        : speaking
            ? Icons.volume_up
            : Icons.mic_none;

    return AnimatedBuilder(
      animation: _pulse,
      builder: (context, child) {
        final scale = listening
            ? 1.0 + _pulse.value * 0.06
            : speaking
                ? 1.0 + _pulse.value * 0.09
                : 1.0;
        return Transform.scale(scale: scale, child: child);
      },
      child: Container(
        width: 160,
        height: 160,
        decoration: BoxDecoration(
          shape: BoxShape.circle,
          color: accent.withValues(alpha: listening ? 0.22 : 0.14),
          border: Border.all(color: accent.withValues(alpha: 0.8), width: 3),
          boxShadow: [
            BoxShadow(
              color: accent.withValues(alpha: 0.3),
              blurRadius: 24,
              spreadRadius: 6,
            ),
          ],
        ),
        child: Icon(icon, color: accent, size: 78),
      ),
    );
  }

  String _hintText() {
    switch (widget.state) {
      case VoiceModeState.idle:
        return widget.inputMode == VoiceInputMode.hold
            ? 'Hold to speak'
            : 'Tap to speak';
      case VoiceModeState.listening:
        return widget.inputMode == VoiceInputMode.hold
            ? 'Listening… release to send'
            : 'Listening… tap to stop';
      case VoiceModeState.transcribing:
        return 'Transcribing…';
      case VoiceModeState.thinking:
        return 'Thinking…';
      case VoiceModeState.speaking:
        return widget.inputMode == VoiceInputMode.hold
            ? 'Speaking… hold to interrupt'
            : 'Speaking… tap to interrupt';
      case VoiceModeState.error:
        return 'Voice mode error';
    }
  }

  (Color, Color, Color) _colorsFor(VoiceModeState state) {
    switch (state) {
      case VoiceModeState.listening:
      case VoiceModeState.transcribing:
        return (const Color(0xFF0D47A1), const Color(0xFF1A237E), Colors.red);
      case VoiceModeState.thinking:
        return (const Color(0xFF4A148C), const Color(0xFF1A0533), Colors.blue);
      case VoiceModeState.speaking:
        return (const Color(0xFF1B5E20), const Color(0xFF003300), Colors.greenAccent);
      case VoiceModeState.error:
        return (const Color(0xFF4E0000), const Color(0xFF1A0000), Colors.redAccent);
      case VoiceModeState.idle:
        return (const Color(0xFF1A237E), const Color(0xFF0D47A1), Colors.lightBlueAccent);
    }
  }
}

class _ErrorPanel extends StatelessWidget {
  const _ErrorPanel({
    required this.message,
    required this.onRetry,
    required this.onOpenSettings,
  });

  final String message;
  final VoidCallback onRetry;
  final VoidCallback onOpenSettings;

  @override
  Widget build(BuildContext context) {
    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        const Icon(Icons.error_outline, color: Colors.white, size: 56),
        const SizedBox(height: 16),
        Text(
          message,
          textAlign: TextAlign.center,
          style: const TextStyle(color: Colors.white, fontSize: 16),
        ),
        const SizedBox(height: 24),
        Row(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            FilledButton.icon(
              onPressed: onRetry,
              icon: const Icon(Icons.refresh),
              label: const Text('Retry'),
            ),
            const SizedBox(width: 12),
            OutlinedButton.icon(
              onPressed: onOpenSettings,
              icon: const Icon(Icons.settings),
              label: const Text('Settings'),
              style: OutlinedButton.styleFrom(foregroundColor: Colors.white),
            ),
          ],
        ),
      ],
    );
  }
}
