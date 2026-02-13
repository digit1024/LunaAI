# [Feature] Qwen TTS (Qween TTS) – switchable TTS provider with streaming and cancellation

## Summary

Add **Qween TTS** (Alibaba Qwen TTS) as an alternative to the built-in device TTS in the mobile app. Users can switch between **Built-in TTS** and **Qween TTS**. When Qween TTS is selected, language is fixed to English; voice, instructions, and API key are configured on a dedicated Qween TTS settings screen. TTS logic is abstracted behind an interface so both providers plug into the existing conversation flow with streaming and cancellation support.

---

## References

- [Speech synthesis - Qwen (usage, models, streaming)](https://www.alibabacloud.com/help/en/model-studio/qwen-tts#80027657a7cm4)
- [Qwen-TTS API reference](https://www.alibabacloud.com/help/en/model-studio/qwen-tts-api)

---

## Requirements

### 1. TTS provider switch (UI: “Qween TTS”)

- In app UI, offer a choice between:
  - **Built-in TTS** (current `flutter_tts`-based behavior)
  - **Qween TTS** (Alibaba Qwen TTS)
- Persist the selected provider (e.g. in preferences).

### 2. Language behavior when Qween TTS is selected

- When **Qween TTS** is selected:
  - **Do not** show language picker for TTS (language is not a user option).
  - Always send `language_type: "English"` (or equivalent) to the Qwen API.
- When **Built-in TTS** is selected:
  - Keep current behavior: user can pick voice language (current “Voice Language” / “Used for both STT and TTS” behavior for TTS only; STT language can remain as-is or follow existing rules).

### 3. Qween TTS defaults and user options

- **Default voice:** `Katerina` (per [supported system voices](https://www.alibabacloud.com/help/en/model-studio/qwen-tts#bac280ddf5a1u)).
- **User-configurable:**
  - **Voice:** pick from Qwen system voices (at least those supported by the chosen model, e.g. `qwen3-tts-flash` or `qwen3-tts-instruct-flash`).
  - **Instructions:** free-text instruction for expressiveness (optional).
- **Default instructions (if user leaves blank):**  
  `"speed: medium, Pitch Medium, Emotion: gentle and bit seductive, characteristics: Magnetic, Usage voice assistant colleague."`  
  (Instruction control is only supported by **Qwen3-TTS-Instruct-Flash**; if using that model, pass `instructions` and optionally `optimize_instructions`.)

### 4. Qween TTS settings screen

- **Dedicated screen** for Qween TTS only (separate from main Setup).
- **Required:** User must provide **API Key** (e.g. `DASHSCOPE_API_KEY`). Store securely (e.g. secure storage / keychain); do not log or expose in UI beyond masked input.
- **Settings to show when Qween TTS is selected:**
  - API Key (required).
  - Voice selector (default: Katerina).
  - Instructions (optional; default text as above).
- Entry point: e.g. from main Setup or from the TTS section (“Qween TTS settings” / “Configure Qween TTS”).

### 5. TTS abstraction and conversation flow

- **Define an interface** (e.g. `TtsProvider` or `ITtsService`) that covers:
  - `speak(text, { onComplete })` (or equivalent)
  - `stop()` / cancel
  - Streaming: consume streamed audio and play (or buffer and play) with cancellation support.
- **Implement two providers:**
  - **Built-in TTS:** current `TtsService` (flutter_tts) wrapped to implement the interface (no streaming; `stop()` and `onComplete` as today).
  - **Qween TTS:** new implementation that:
    - Calls Alibaba’s **streaming** TTS API (e.g. `stream=True`, `X-DashScope-SSE: enable`).
    - Feeds received Base64 PCM (e.g. 24 kHz, 16-bit mono) into an audio pipeline (e.g. `audioplayers` or a low-latency stream player).
    - Supports **cancellation:** when `stop()` is called, cancel the HTTP/SSE request and stop playback immediately.
- **Embed in current conversation flow:**
  - Chat screen (and any other place that triggers TTS) uses the **selected provider** from preferences (Built-in vs Qween) and calls only the interface (speak/stop/stream + onComplete).
  - No duplicate TTS logic: one code path that uses the abstraction (e.g. `ref.read(ttsProviderResolver).getActiveProvider()` → `speak` / `stop`).

### 6. Streaming and cancellation

- **Streaming:** Use Qwen TTS **streaming** endpoint so playback can start as soon as the first chunks are available; avoid waiting for the full file when Qween TTS is selected.
- **Cancellation:**
  - User stops TTS (e.g. play/stop in bubble or “stop speaking” in voice mode) → call `stop()` on the active provider.
  - When starting a new TTS (e.g. new message), cancel any ongoing TTS (current and Qween) before starting the new one.
  - If the user switches to listening (e.g. starts speaking) during Qween TTS playback, cancel the stream and stop playback (same as today for built-in TTS).

---

## Technical notes (from Alibaba docs)

- **Streaming:** POST with `X-DashScope-SSE: enable`; response is SSE with Base64 PCM chunks; sample rate 24 kHz.
- **International endpoint (Singapore):** `https://dashscope-intl.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation`
- **Instruction control:** Only for model `qwen3-tts-instruct-flash`; parameters `instructions` and `optimize_instructions`.
- **Voice:** e.g. `Katerina`, `Cherry`, etc. (see [supported system voices](https://www.alibabacloud.com/help/en/model-studio/qwen-tts#bac280ddf5a1u)).

---

## Implementation plan (high level)

| # | Step | Scope |
|---|------|--------|
| 1 | **TTS abstraction** | Add `TtsProvider` interface (e.g. `speak`, `stop`, optional stream callback / `onComplete`). Implement `BuiltInTtsProvider` wrapping current `TtsService`. |
| 2 | **Preferences** | Extend TTS preferences (or add a small “provider” prefs): provider type (built-in vs Qween), and optionally “last used” for Qween. Add Qween-specific preferences: API key (secure), voice, instructions, model (if we allow it). |
| 3 | **Qween TTS settings screen** | New screen: API Key, voice dropdown (default Katerina), instructions field (default text as above). Save to secure storage + prefs. Validate API key (e.g. optional test call or just format check). |
| 4 | **Qween TTS client** | Implement HTTP/SSE client for Qwen streaming TTS: auth (Bearer API key), request body (model, text, voice, language_type, instructions if model supports it), parse SSE, decode Base64 PCM, feed to audio. |
| 5 | **Qween TTS provider** | Implement `QweenTtsProvider` implementing `TtsProvider`: uses client from (4), supports `speak` (streaming), `stop()` (cancel request + stop playback). Wire API key and options from Qween settings. |
| 6 | **Provider resolution** | Resolver (e.g. provider) that returns the active `TtsProvider` based on preferences (built-in vs Qween). Chat screen and chat bubble use this resolver instead of `ttsServiceProvider` directly for “speak” and “stop”. |
| 7 | **UI: TTS provider switch** | In Setup (and/or chat menu): “TTS provider” or “Voice engine”: Built-in TTS | Qween TTS. When Qween TTS: hide TTS language picker; show “Qween TTS settings” entry. |
| 8 | **Conversation flow** | Ensure `_playTtsForMessage`, `_TtsPlayButton`, and any “stop TTS” path use the resolver and call interface only; no `setLanguage` when provider is Qween (language fixed to English for that provider). |
| 9 | **Cancellation** | On “stop TTS”, new message TTS, or “user started speaking”: call `stop()` on current provider; Qween implementation cancels HTTP/SSE and stops audio. |
| 10 | **Testing** | Manual: switch provider, play message with Built-in and Qween, cancel mid-stream, change voice/instructions, API key validation. |

---

## Acceptance criteria

- [ ] User can select “Built-in TTS” or “Qween TTS” in app settings; selection is persisted.
- [ ] When Qween TTS is selected, TTS language picker is hidden; Qwen API is always called with English.
- [ ] Default Qween voice is Katerina; user can change voice and instructions on the Qween TTS settings screen.
- [ ] Default instructions are: “speed: medium, Pitch Medium, Emotion: gentle and bit seductive, characteristics: Magnetic, Usage voice assistant colleague.”
- [ ] Dedicated “Qween TTS settings” screen: API Key (required), voice, instructions; API key stored securely.
- [ ] TTS in conversation (auto-play and play button on bubble) uses the selected provider via a single interface; no duplicate TTS logic.
- [ ] Qween TTS uses streaming (playback starts from first chunks); cancellation (stop button, new message, user speaks) stops stream and playback immediately.
- [ ] Built-in TTS behavior unchanged when selected (including language and existing flows).

---

## Labels / metadata (suggested)

- **Component:** `mobile_app`
- **Type:** feature
- **Backlog** → to be moved to **Ready** after architecture review (Architect agent).
