package com.luna.mobile.wear

import android.app.Activity
import android.content.Intent
import android.speech.RecognizerIntent
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel

class MainActivity: FlutterActivity() {
    private val CHANNEL = "com.luna.mobile.wear/speech"
    private val SPEECH_REQUEST_CODE = 100
    private var pendingResult: MethodChannel.Result? = null

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, CHANNEL).setMethodCallHandler { call, result ->
            when (call.method) {
                "startSpeechRecognition" -> {
                    val language = call.argument<String>("language") ?: "en-US"
                    val prompt = call.argument<String>("prompt") ?: "Speak now"
                    startSpeechRecognition(language, prompt, result)
                }
                "isSpeechAvailable" -> {
                    result.success(isSpeechRecognitionAvailable())
                }
                else -> {
                    result.notImplemented()
                }
            }
        }
    }

    private fun isSpeechRecognitionAvailable(): Boolean {
        val intent = Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH)
        val activities = packageManager.queryIntentActivities(intent, 0)
        return activities.isNotEmpty()
    }

    private fun startSpeechRecognition(language: String, prompt: String, result: MethodChannel.Result) {
        pendingResult = result
        
        val intent = Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH).apply {
            putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
            putExtra(RecognizerIntent.EXTRA_LANGUAGE, language)
            putExtra(RecognizerIntent.EXTRA_PROMPT, prompt)
            putExtra(RecognizerIntent.EXTRA_MAX_RESULTS, 1)
        }
        
        try {
            startActivityForResult(intent, SPEECH_REQUEST_CODE)
        } catch (e: Exception) {
            pendingResult?.error("SPEECH_ERROR", "Failed to start speech recognition: ${e.message}", null)
            pendingResult = null
        }
    }

    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        
        if (requestCode == SPEECH_REQUEST_CODE) {
            when (resultCode) {
                Activity.RESULT_OK -> {
                    val results = data?.getStringArrayListExtra(RecognizerIntent.EXTRA_RESULTS)
                    val spokenText = results?.firstOrNull() ?: ""
                    pendingResult?.success(mapOf(
                        "success" to true,
                        "text" to spokenText
                    ))
                }
                Activity.RESULT_CANCELED -> {
                    pendingResult?.success(mapOf(
                        "success" to false,
                        "text" to "",
                        "cancelled" to true
                    ))
                }
                else -> {
                    pendingResult?.success(mapOf(
                        "success" to false,
                        "text" to "",
                        "error" to "Recognition failed"
                    ))
                }
            }
            pendingResult = null
        }
    }
}
