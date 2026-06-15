package com.luna.mobile

import android.app.Activity
import android.content.Intent
import android.os.Handler
import android.os.Looper
import android.speech.RecognizerIntent
import com.google.android.gms.wearable.MessageClient
import com.google.android.gms.wearable.Wearable
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.EventChannel
import io.flutter.plugin.common.MethodChannel

class MainActivity : FlutterActivity() {
    private val SPEECH_CHANNEL = "com.luna.mobile.wear/speech"
    private val WEAR_SYNC_CHANNEL = "com.luna.mobile/wear_sync"
    // Receives config pushed from the phone — used when this APK runs on the watch
    private val CONFIG_SYNC_CHANNEL = "com.luna.mobile.wear/config_sync"
    private val SPEECH_REQUEST_CODE = 100
    private var pendingResult: MethodChannel.Result? = null
    private val mainHandler = Handler(Looper.getMainLooper())

    private var configEventSink: EventChannel.EventSink? = null
    private val messageListener = MessageClient.OnMessageReceivedListener { event ->
        if (event.path == "/luna/config") {
            val json = String(event.data, Charsets.UTF_8)
            mainHandler.post { configEventSink?.success(json) }
        }
    }

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)

        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, SPEECH_CHANNEL).setMethodCallHandler { call, result ->
            when (call.method) {
                "startSpeechRecognition" -> {
                    val language = call.argument<String>("language") ?: "en-US"
                    val prompt = call.argument<String>("prompt") ?: "Speak now"
                    startSpeechRecognition(language, prompt, result)
                }
                "isSpeechAvailable" -> {
                    result.success(isSpeechRecognitionAvailable())
                }
                else -> result.notImplemented()
            }
        }

        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, WEAR_SYNC_CHANNEL).setMethodCallHandler { call, result ->
            when (call.method) {
                "sendConfigToWatch" -> {
                    val host = call.argument<String>("host") ?: ""
                    val port = call.argument<Int>("port") ?: 8080
                    val apiKey = call.argument<String>("apiKey") ?: ""
                    sendConfigToWear(host, port, apiKey, result)
                }
                else -> result.notImplemented()
            }
        }

        // EventChannel for receiving config from phone — active when this APK runs on the watch
        EventChannel(flutterEngine.dartExecutor.binaryMessenger, CONFIG_SYNC_CHANNEL)
            .setStreamHandler(object : EventChannel.StreamHandler {
                override fun onListen(arguments: Any?, events: EventChannel.EventSink?) {
                    configEventSink = events
                }
                override fun onCancel(arguments: Any?) {
                    configEventSink = null
                }
            })
    }

    override fun onResume() {
        super.onResume()
        Wearable.getMessageClient(this).addListener(messageListener)
    }

    override fun onPause() {
        super.onPause()
        Wearable.getMessageClient(this).removeListener(messageListener)
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

    private fun sendConfigToWear(host: String, port: Int, apiKey: String, result: MethodChannel.Result) {
        val json = """{"host":"$host","port":$port,"apiKey":"$apiKey"}"""

        Wearable.getNodeClient(this).connectedNodes
            .addOnSuccessListener { nodes ->
                if (nodes.isEmpty()) {
                    mainHandler.post {
                        result.success(mapOf("success" to false, "error" to "No watch connected via Bluetooth"))
                    }
                    return@addOnSuccessListener
                }

                var pending = nodes.size
                var anySuccess = false

                for (node in nodes) {
                    Wearable.getMessageClient(this)
                        .sendMessage(node.id, "/luna/config", json.toByteArray(Charsets.UTF_8))
                        .addOnSuccessListener {
                            anySuccess = true
                            pending--
                            if (pending == 0) {
                                mainHandler.post {
                                    result.success(mapOf("success" to true))
                                }
                            }
                        }
                        .addOnFailureListener { e ->
                            pending--
                            if (pending == 0) {
                                mainHandler.post {
                                    if (anySuccess) result.success(mapOf("success" to true))
                                    else result.success(mapOf("success" to false, "error" to (e.message ?: "Send failed")))
                                }
                            }
                        }
                }
            }
            .addOnFailureListener { e ->
                mainHandler.post {
                    result.success(mapOf("success" to false, "error" to (e.message ?: "Could not get connected nodes")))
                }
            }
    }

    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)

        if (requestCode == SPEECH_REQUEST_CODE) {
            when (resultCode) {
                Activity.RESULT_OK -> {
                    val results = data?.getStringArrayListExtra(RecognizerIntent.EXTRA_RESULTS)
                    val spokenText = results?.firstOrNull() ?: ""
                    pendingResult?.success(mapOf("success" to true, "text" to spokenText))
                }
                Activity.RESULT_CANCELED -> {
                    pendingResult?.success(mapOf("success" to false, "text" to "", "cancelled" to true))
                }
                else -> {
                    pendingResult?.success(mapOf("success" to false, "text" to "", "error" to "Recognition failed"))
                }
            }
            pendingResult = null
        }
    }
}
