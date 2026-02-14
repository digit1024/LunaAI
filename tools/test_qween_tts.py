#!/usr/bin/env python3
"""
Qwen TTS API probe — tests both non-streaming and streaming (SSE) responses,
logs everything so we can see the exact response format.

Usage:
  python3 test_qween_tts.py <API_KEY>
  python3 test_qween_tts.py <API_KEY> "Custom text to speak"
"""

import sys
import json
import base64
import requests
from pathlib import Path

ENDPOINT = "https://dashscope-intl.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation"
MODEL = "qwen3-tts-instruct-flash"
VOICE = "Katerina"
TEXT = "Hello! This is a test of the Qween TTS system. How does it sound?"
OUT_DIR = Path(__file__).parent / "tts_output"

INSTRUCTIONS = "speed: medium, Pitch Medium, Emotion: gentle"


def test_non_streaming(api_key: str, text: str):
    """Non-streaming POST — full response in one shot."""
    print("\n" + "=" * 60)
    print("TEST 1: NON-STREAMING (no SSE, no response_format)")
    print("=" * 60)

    body = {
        "model": MODEL,
        "input": {
            "text": text,
            "voice": VOICE,
            "language_type": "English",
        },
        "parameters": {
            "instructions": INSTRUCTIONS,
            "optimize_instructions": True,
        },
    }

    headers = {
        "Authorization": f"Bearer {api_key}",
        "Content-Type": "application/json",
    }

    print(f"\n→ POST {ENDPOINT}")
    print(f"  model={MODEL}  voice={VOICE}")
    print(f"  text={text[:80]}...")

    resp = requests.post(ENDPOINT, headers=headers, json=body, timeout=30)

    print(f"\n← status: {resp.status_code}")
    print(f"← content-type: {resp.headers.get('content-type')}")
    print(f"← content-length: {resp.headers.get('content-length')}")
    print(f"← body size: {len(resp.content)} bytes")

    ct = resp.headers.get("content-type", "")

    if "audio/" in ct:
        # Direct audio bytes
        out = OUT_DIR / "non_stream.mp3"
        out.write_bytes(resp.content)
        print(f"\n✓ Got direct audio bytes → saved to {out}")
        print(f"  Size: {len(resp.content)} bytes")
        return

    # JSON response
    print(f"\n← body (first 2000 chars):")
    body_text = resp.text[:2000]
    print(body_text)

    # Save full response
    (OUT_DIR / "non_stream_response.json").write_text(resp.text)
    print(f"\n  Full response saved to {OUT_DIR / 'non_stream_response.json'}")

    if resp.status_code != 200:
        print(f"\n✗ Error response")
        return

    try:
        data = resp.json()
        print(f"\n  Top-level keys: {list(data.keys())}")

        if "output" in data:
            output = data["output"]
            print(f"  output keys: {list(output.keys())}")

            # Walk all paths and find any base64-looking strings
            _walk_and_extract(output, "output", 0)

    except json.JSONDecodeError as e:
        print(f"\n✗ Not valid JSON: {e}")


def test_streaming(api_key: str, text: str):
    """Streaming SSE — X-DashScope-SSE: enable."""
    print("\n" + "=" * 60)
    print("TEST 2: STREAMING (SSE, no response_format)")
    print("=" * 60)

    body = {
        "model": MODEL,
        "input": {
            "text": text,
            "voice": VOICE,
            "language_type": "English",
        },
        "parameters": {
            "instructions": INSTRUCTIONS,
            "optimize_instructions": True,
        },
    }

    headers = {
        "Authorization": f"Bearer {api_key}",
        "Content-Type": "application/json",
        "X-DashScope-SSE": "enable",
    }

    print(f"\n→ POST {ENDPOINT}  (SSE)")
    print(f"  model={MODEL}  voice={VOICE}  (no response_format, no stream field)")

    resp = requests.post(ENDPOINT, headers=headers, json=body, stream=True, timeout=30)

    print(f"\n← status: {resp.status_code}")
    print(f"← content-type: {resp.headers.get('content-type')}")

    if resp.status_code != 200:
        print(f"← error: {resp.text[:1000]}")
        (OUT_DIR / "stream_error.txt").write_text(resp.text)
        return

    lines = []
    audio_chunks = []
    chunk_count = 0

    for raw_line in resp.iter_lines(decode_unicode=True):
        if raw_line is None:
            continue
        line = raw_line.strip() if isinstance(raw_line, str) else raw_line.decode().strip()
        lines.append(line)

        # Log first 20 lines fully, then summarize
        if len(lines) <= 20:
            display = line[:300] if len(line) > 300 else line
            print(f"  [{len(lines):3d}] {display}")
        elif len(lines) == 21:
            print(f"  ... (logging remaining to file) ...")

        if not line.startswith("data:"):
            continue

        data_str = line[5:].strip()
        if data_str == "[DONE]" or not data_str:
            continue

        try:
            data = json.loads(data_str)

            if chunk_count == 0:
                # Log full structure of first chunk
                print(f"\n  First SSE data chunk keys: {list(data.keys())}")
                if "output" in data:
                    print(f"  output keys: {list(data['output'].keys())}")
                    _walk_and_extract(data["output"], "output", 0, extract=False)
                if "choices" in data:
                    print(f"  choices: {json.dumps(data['choices'][:1], indent=2)[:500]}")

            # Try to extract audio from any known path
            audio_b64 = _find_audio_in_json(data)
            if audio_b64:
                audio_bytes = base64.b64decode(audio_b64)
                audio_chunks.append(audio_bytes)
                chunk_count += 1

        except json.JSONDecodeError:
            pass

    # Save raw SSE log
    (OUT_DIR / "stream_sse_lines.txt").write_text("\n".join(lines))
    print(f"\n  Total SSE lines: {len(lines)}")
    print(f"  Audio chunks extracted: {chunk_count}")
    print(f"  Raw SSE log saved to {OUT_DIR / 'stream_sse_lines.txt'}")

    if audio_chunks:
        combined = b"".join(audio_chunks)
        out = OUT_DIR / "stream_audio.pcm"
        out.write_bytes(combined)
        print(f"  Combined audio: {len(combined)} bytes → saved to {out}")


def test_voice_comparison(api_key: str):
    """Test multiple voices on BOTH models to see which one respects voice param."""
    print("\n" + "=" * 60)
    print("TEST: VOICE COMPARISON (instruct-flash vs flash)")
    print("=" * 60)

    voices_to_test = ["Katerina", "Cherry", "Ethan", "Chelsie"]
    models_to_test = [
        "qwen3-tts-instruct-flash",
        "qwen3-tts-flash",
    ]
    short_text = "Hello, how are you today?"

    for model in models_to_test:
        print(f"\n  --- Model: {model} ---")
        for voice in voices_to_test:
            body = {
                "model": model,
                "input": {
                    "text": short_text,
                    "voice": voice,
                    "language_type": "English",
                },
            }
            # Add instructions only for instruct model
            if "instruct" in model:
                body["parameters"] = {
                    "instructions": "Normal speaking pace.",
                    "optimize_instructions": True,
                }
        headers = {
            "Authorization": f"Bearer {api_key}",
            "Content-Type": "application/json",
        }

        try:
            resp = requests.post(ENDPOINT, headers=headers, json=body, timeout=15)
            if resp.status_code == 200:
                data = resp.json()
                audio = data.get("output", {}).get("audio", {})
                url = audio.get("url", "")
                audio_id = audio.get("id", "")
                print(f"  ✓ {voice:20s}  status=200  id={audio_id[-12:]}  url={'yes' if url else 'no'}")

                # Download and save to compare file sizes
                if url:
                    wav_resp = requests.get(url, timeout=10)
                    if wav_resp.status_code == 200:
                        suffix = "instruct" if "instruct" in model else "flash"
                        out = OUT_DIR / f"voice_{suffix}_{voice}.wav"
                        out.write_bytes(wav_resp.content)
                        print(f"    → {out.name}  {len(wav_resp.content)} bytes")
            else:
                err = resp.json().get("message", resp.text[:200])
                print(f"  ✗ {voice:20s}  status={resp.status_code}  {err[:100]}")
        except Exception as e:
            print(f"  ✗ {voice:20s}  error: {e}")

    print(f"\n  Compare WAV files in {OUT_DIR}/ — different sizes = different voices")


def test_non_streaming_pcm(api_key: str, text: str):
    """Non-streaming with sample_rate only (no response_format)."""
    print("\n" + "=" * 60)
    print("TEST 3: NON-STREAMING (with sample_rate=24000)")
    print("=" * 60)

    body = {
        "model": MODEL,
        "input": {
            "text": text,
            "voice": VOICE,
            "language_type": "English",
        },
        "parameters": {
            "sample_rate": 24000,
            "instructions": INSTRUCTIONS,
            "optimize_instructions": True,
        },
    }

    headers = {
        "Authorization": f"Bearer {api_key}",
        "Content-Type": "application/json",
    }

    print(f"\n→ POST {ENDPOINT}")
    print(f"  sample_rate=24000 (no SSE, no response_format)")

    resp = requests.post(ENDPOINT, headers=headers, json=body, timeout=30)

    print(f"\n← status: {resp.status_code}")
    print(f"← content-type: {resp.headers.get('content-type')}")
    print(f"← body size: {len(resp.content)} bytes")

    ct = resp.headers.get("content-type", "")

    if "audio/" in ct or "application/octet-stream" in ct:
        out = OUT_DIR / "non_stream.pcm"
        out.write_bytes(resp.content)
        print(f"\n✓ Got direct audio/binary → saved to {out}")
        return

    print(f"\n← body (first 2000 chars):")
    print(resp.text[:2000])
    (OUT_DIR / "non_stream_pcm_response.json").write_text(resp.text)


def test_non_streaming_wav(api_key: str, text: str):
    """Non-streaming with basic qwen3-tts-flash (non-instruct) model."""
    print("\n" + "=" * 60)
    print("TEST 4: NON-STREAMING (qwen3-tts-flash, no instruct)")
    print("=" * 60)

    body = {
        "model": "qwen3-tts-flash",
        "input": {
            "text": text,
            "voice": VOICE,
            "language_type": "English",
        },
    }

    headers = {
        "Authorization": f"Bearer {api_key}",
        "Content-Type": "application/json",
    }

    print(f"\n→ POST {ENDPOINT}")
    print(f"  model=qwen3-tts-flash  (minimal params)")

    resp = requests.post(ENDPOINT, headers=headers, json=body, timeout=30)

    print(f"\n← status: {resp.status_code}")
    print(f"← content-type: {resp.headers.get('content-type')}")
    print(f"← body size: {len(resp.content)} bytes")

    ct = resp.headers.get("content-type", "")

    if "audio/" in ct or "application/octet-stream" in ct:
        out = OUT_DIR / "non_stream.wav"
        out.write_bytes(resp.content)
        print(f"\n✓ Got direct audio/binary → saved to {out}")
        return

    print(f"\n← body (first 2000 chars):")
    print(resp.text[:2000])
    (OUT_DIR / "non_stream_wav_response.json").write_text(resp.text)


# ── Helpers ──────────────────────────────────────────────────────

def _walk_and_extract(obj, path: str, depth: int, extract: bool = True):
    """Recursively walk JSON and report structure + extract base64 audio."""
    indent = "  " * (depth + 2)
    if isinstance(obj, dict):
        for key, val in obj.items():
            p = f"{path}.{key}"
            if isinstance(val, str):
                is_b64 = len(val) > 100 and all(c in "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=\n" for c in val[:200])
                if is_b64 and extract:
                    print(f"{indent}{p}: <base64 string, {len(val)} chars>")
                    try:
                        decoded = base64.b64decode(val)
                        out = OUT_DIR / f"extracted_{key}.bin"
                        out.write_bytes(decoded)
                        print(f"{indent}  → decoded {len(decoded)} bytes → {out}")
                    except Exception as e:
                        print(f"{indent}  → decode failed: {e}")
                else:
                    preview = val[:80] if len(val) > 80 else val
                    print(f"{indent}{p}: \"{preview}\" ({len(val)} chars)")
            elif isinstance(val, (int, float, bool)):
                print(f"{indent}{p}: {val}")
            elif isinstance(val, list):
                print(f"{indent}{p}: list[{len(val)}]")
                for i, item in enumerate(val[:3]):
                    _walk_and_extract(item, f"{p}[{i}]", depth + 1, extract)
            elif isinstance(val, dict):
                print(f"{indent}{p}: dict{{{','.join(val.keys())}}}")
                _walk_and_extract(val, p, depth + 1, extract)
            elif val is None:
                print(f"{indent}{p}: null")
    elif isinstance(obj, list):
        for i, item in enumerate(obj[:3]):
            _walk_and_extract(item, f"{path}[{i}]", depth, extract)


def _find_audio_in_json(data: dict) -> str | None:
    """Try to find base64 audio in a JSON response using multiple paths."""
    # output.audio
    output = data.get("output", {})
    if isinstance(output, dict):
        audio = output.get("audio")
        if audio and isinstance(audio, str):
            return audio

    # output.choices[0].message.content[*].audio
    choices = output.get("choices") if isinstance(output, dict) else data.get("choices")
    if isinstance(choices, list) and choices:
        msg = choices[0].get("message", {})
        content = msg.get("content")
        if isinstance(content, list):
            for item in content:
                if isinstance(item, dict):
                    a = item.get("audio") or item.get("audio_content")
                    if a:
                        return a
        elif isinstance(content, str) and len(content) > 100:
            return content

    # delta (streaming)
    delta = data.get("delta")
    if isinstance(delta, str) and len(delta) > 50:
        return delta

    # output.preview_audio.data
    if isinstance(output, dict):
        pa = output.get("preview_audio", {})
        if isinstance(pa, dict):
            return pa.get("data")

    return None


def main():
    if len(sys.argv) < 2:
        print("Usage: python3 test_qween_tts.py <API_KEY> [text]")
        sys.exit(1)

    api_key = sys.argv[1]
    text = sys.argv[2] if len(sys.argv) > 2 else TEXT

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    print(f"Output directory: {OUT_DIR}")

    test_non_streaming(api_key, text)
    test_streaming(api_key, text)
    test_voice_comparison(api_key)
    test_non_streaming_pcm(api_key, text)
    test_non_streaming_wav(api_key, text)

    print("\n" + "=" * 60)
    print("DONE — check the output files:")
    for f in sorted(OUT_DIR.iterdir()):
        print(f"  {f.name}  ({f.stat().st_size} bytes)")
    print("=" * 60)


if __name__ == "__main__":
    main()
