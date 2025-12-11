import 'dart:async';
import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:speech_to_text/speech_to_text.dart' as stt;
import '../utils/platform_utils.dart';

final speechServiceProvider = Provider<SpeechService>((ref) {
  final service = SpeechService();
  return service;
});

class SpeechService {
  final stt.SpeechToText _speech = stt.SpeechToText();
  bool _initialized = false;
  bool _isListening = false;
  bool _hasPendingText = false; // Track if we have text that hasn't been sent
  bool _pauseAlreadyTriggered = false; // Prevent double-triggering
  bool _sessionActive = false; // Track if we're in an active voice session
  int _sessionId = 0; // Incremented each session to invalidate stale callbacks
  Timer? _pauseTimer;
  String _currentText = '';
  
  // Callbacks
  Function(String text)? onResult;
  Function()? onPauseDetected;
  Function(String error)? onError;
  Function()? onUnexpectedStop; // Called when STT stops unexpectedly

  Duration _pauseDuration = const Duration(seconds: 2);
  
  /// Set the pause duration for STT pause detection
  void setPauseDuration(Duration duration) {
    _pauseDuration = duration;
  }

  /// Initialize speech recognition
  Future<bool> initialize() async {
    if (_initialized) return true;
    
    // STT only available on mobile platforms
    if (!isMobile) {
      debugPrint('SpeechService: Not available on desktop/web platform');
      _initialized = false;
      return false;
    }
    
    debugPrint('SpeechService: Initializing...');
    
    final available = await _speech.initialize(
      onError: (error) {
        debugPrint('SpeechService: Error - ${error.errorMsg}');
        onError?.call(error.errorMsg);
      },
      onStatus: (status) {
        debugPrint('SpeechService: Status changed to "$status", _isListening=$_isListening, _sessionActive=$_sessionActive, _hasPendingText=$_hasPendingText, text="$_currentText"');
        
        if (status == 'listening') {
          _isListening = true;
        } else if (status == 'done' || status == 'notListening') {
          final wasListening = _isListening;
          _isListening = false;
          
          // Only process if session is still active
          if (!_sessionActive) {
            debugPrint('SpeechService: Session not active, ignoring status change');
            return;
          }
          
          // When speech recognition ends and we have pending text, trigger send
          if (_hasPendingText && _currentText.trim().isNotEmpty && !_pauseAlreadyTriggered) {
            debugPrint('SpeechService: Speech ended with pending text, triggering onPauseDetected');
            _hasPendingText = false;
            _pauseAlreadyTriggered = true;
            _pauseTimer?.cancel();
            _pauseTimer = null;
            final currentSessionId = _sessionId;
            // Small delay to ensure final result is processed
            Future.delayed(const Duration(milliseconds: 100), () {
              // Verify session is still valid
              if (_sessionId == currentSessionId && _sessionActive) {
                debugPrint('SpeechService: Calling onPauseDetected callback');
                onPauseDetected?.call();
              } else {
                debugPrint('SpeechService: Session changed, skipping onPauseDetected');
              }
            });
          } else if (wasListening && _sessionActive && !_hasPendingText) {
            // STT stopped but no text - might be an unexpected stop
            debugPrint('SpeechService: STT stopped unexpectedly with no pending text');
            onUnexpectedStop?.call();
          }
        }
      },
    );
    
    _initialized = available;
    debugPrint('SpeechService: Initialized, available=$available');
    return available;
  }

  /// Check if speech recognition is available
  Future<bool> isAvailable() async {
    // Desktop/web: STT not available
    if (!isMobile) {
      return false;
    }
    
    if (!_initialized) {
      await initialize();
    }
    return _initialized;
  }

  /// Start listening for speech
  Future<bool> startListening(String languageCode, {Duration? pauseDuration}) async {
    // Desktop/web: STT not available
    if (!isMobile) {
      debugPrint('SpeechService: startListening called on desktop/web - not available');
      return false;
    }
    
    // Update pause duration if provided
    if (pauseDuration != null) {
      _pauseDuration = pauseDuration;
    }
    debugPrint('SpeechService: startListening called with language=$languageCode');
    
    if (!_initialized) {
      final initialized = await initialize();
      if (!initialized) {
        debugPrint('SpeechService: Failed to initialize');
        return false;
      }
    }

    if (_isListening) {
      debugPrint('SpeechService: Already listening, stopping first');
      await stopListening();
    }

    // Clear previous text when starting a new listening session
    _currentText = '';
    _hasPendingText = false;
    _pauseAlreadyTriggered = false; // Reset for new session
    _sessionActive = true;
    _sessionId++; // Invalidate any pending callbacks from previous sessions
    _isListening = true;
    
    debugPrint('SpeechService: New session started, sessionId=$_sessionId');

    debugPrint('SpeechService: Starting speech.listen()');
    
    final result = await _speech.listen(
      onResult: (result) {
        _currentText = result.recognizedWords;
        _hasPendingText = _currentText.trim().isNotEmpty;
        
        debugPrint('SpeechService: onResult - text="$_currentText", final=${result.finalResult}');
        
        if (result.finalResult) {
          // Final result received - trigger pause detection after a short delay
          // This is a backup in case onStatus doesn't fire reliably
          debugPrint('SpeechService: Final result received, scheduling pause detection');
          _pauseTimer?.cancel();
          if (_hasPendingText && _currentText.trim().isNotEmpty && !_pauseAlreadyTriggered) {
            final currentSessionId = _sessionId;
            _pauseTimer = Timer(const Duration(milliseconds: 500), () {
              // Verify session is still valid
              if (_sessionId != currentSessionId || !_sessionActive) {
                debugPrint('SpeechService: Session changed, skipping finalResult timer');
                return;
              }
              if (_hasPendingText && _currentText.trim().isNotEmpty && !_pauseAlreadyTriggered) {
                debugPrint('SpeechService: Triggering onPauseDetected from finalResult timer');
                _hasPendingText = false;
                _pauseAlreadyTriggered = true;
                onPauseDetected?.call();
              }
            });
          }
        } else {
          // Partial result, reset pause timer on each update
          _resetPauseTimer();
        }
        
        onResult?.call(_currentText);
      },
      localeId: languageCode,
      listenOptions: stt.SpeechListenOptions(
        listenMode: stt.ListenMode.dictation,
        cancelOnError: false,
        partialResults: true,
      ),
    );

    debugPrint('SpeechService: speech.listen() returned $result');
    return result;
  }

  /// Stop listening
  Future<void> stopListening() async {
    debugPrint('SpeechService: stopListening called');
    if (!isMobile) return; // Desktop: No-op
    _isListening = false;
    _pauseTimer?.cancel();
    _pauseTimer = null;
    await _speech.stop();
  }

  /// Cancel listening (without processing results)
  Future<void> cancel() async {
    debugPrint('SpeechService: cancel called');
    if (!isMobile) {
      // Desktop: Just clear state
      _sessionActive = false;
      _isListening = false;
      _hasPendingText = false;
      _pauseAlreadyTriggered = false;
      _pauseTimer?.cancel();
      _pauseTimer = null;
      _currentText = '';
      return;
    }
    _sessionActive = false; // Mark session as inactive to prevent stale callbacks
    _isListening = false;
    _hasPendingText = false;
    _pauseAlreadyTriggered = false;
    _pauseTimer?.cancel();
    _pauseTimer = null;
    await _speech.cancel();
    _currentText = '';
  }
  
  /// Clear all callbacks (call when exiting voice mode)
  void clearCallbacks() {
    debugPrint('SpeechService: Clearing callbacks');
    onResult = null;
    onPauseDetected = null;
    onError = null;
    onUnexpectedStop = null;
  }

  /// Reset the pause detection timer
  void _resetPauseTimer() {
    _pauseTimer?.cancel();
    final currentSessionId = _sessionId;
    _pauseTimer = Timer(_pauseDuration, () {
      // Verify session is still valid
      if (_sessionId != currentSessionId || !_sessionActive) {
        debugPrint('SpeechService: Session changed, skipping pause timer');
        return;
      }
      debugPrint('SpeechService: Pause timer fired, _hasPendingText=$_hasPendingText, _pauseAlreadyTriggered=$_pauseAlreadyTriggered, text="$_currentText"');
      // Timer fires after pause - check if we have text to send
      if (_hasPendingText && _currentText.trim().isNotEmpty && !_pauseAlreadyTriggered) {
        debugPrint('SpeechService: Triggering onPauseDetected from timer');
        _hasPendingText = false;
        _pauseAlreadyTriggered = true;
        onPauseDetected?.call();
      }
    });
  }

  /// Get current recognized text
  String get currentText => _currentText;

  /// Check if currently listening
  bool get isListening => _isListening;

  /// Dispose resources
  void dispose() {
    _pauseTimer?.cancel();
    if (isMobile) {
      _speech.cancel();
    }
    _isListening = false;
  }
}

