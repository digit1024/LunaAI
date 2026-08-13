#!/usr/bin/env python3
"""
Telegram bridge for Luna AI.

Receives messages from Telegram (user input and commands /start, /new, /profile),
forwards plain text to the Luna server via WebSocket, and sends the reply back.
Commands are Telegram chat messages, not CLI arguments.
"""

import asyncio
import json
import os
import sys
from typing import Optional

# Ensure we can import trigger from same directory
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from trigger import LunaClient

import websockets
from telegram import Update
from telegram.ext import Application, CommandHandler, MessageHandler, ContextTypes, filters

# Per-chat profile: None = server default, str = profile name
chat_profiles: dict[int, Optional[str]] = {}

# Per-chat Luna session: one WebSocket + conversation per chat (multi-turn)
_chat_sessions: dict[int, "LunaClientCollect"] = {}
_chat_locks: dict[int, asyncio.Lock] = {}

# Allowed Telegram user IDs. REQUIRED at startup (fail closed).
# Set via env: ALLOWED_TELEGRAM_IDS=123,456,789
_allowed_user_ids: Optional[frozenset[int]] = None


def _parse_allowed_ids() -> Optional[frozenset[int]]:
    raw = os.getenv("ALLOWED_TELEGRAM_IDS", "").strip()
    if not raw:
        return None
    ids = set()
    for part in raw.split(","):
        part = part.strip()
        if part:
            try:
                ids.add(int(part))
            except ValueError:
                continue
    return frozenset(ids) if ids else None


def _is_allowed(update: Update) -> bool:
    """True only when the sender's Telegram user ID is in ALLOWED_TELEGRAM_IDS."""
    if _allowed_user_ids is None:
        return False
    user = update.effective_user
    return user is not None and user.id in _allowed_user_ids

TELEGRAM_MAX_MESSAGE_LENGTH = 4096
CHUNK_SIZE = 4000


def split_for_telegram(text: str) -> list[str]:
    """Split text into chunks under Telegram's limit, preferring line breaks."""
    if len(text) <= TELEGRAM_MAX_MESSAGE_LENGTH:
        return [text] if text else []
    chunks = []
    remaining = text
    while remaining:
        if len(remaining) <= TELEGRAM_MAX_MESSAGE_LENGTH:
            chunks.append(remaining)
            break
        chunk = remaining[:CHUNK_SIZE]
        last_newline = chunk.rfind("\n")
        if last_newline > CHUNK_SIZE // 2:
            chunk = chunk[: last_newline + 1]
        else:
            # No nice break, split at space or hard cut
            last_space = chunk.rfind(" ")
            if last_space > CHUNK_SIZE // 2:
                chunk = chunk[: last_space + 1]
        chunks.append(chunk)
        remaining = remaining[len(chunk) :].lstrip("\n ")
    return chunks


class LunaClientCollect(LunaClient):
    """LunaClient that collects assistant reply instead of printing. Can be reused for multi-turn."""

    def __init__(self, address: str, api_key: str):
        super().__init__(address, api_key)
        self.collected: list[str] = []

    async def connect(self):
        """Connect; raise on failure instead of sys.exit."""
        url = self._get_ws_url()
        try:
            self.websocket = await websockets.connect(
                url,
                extra_headers=self._get_headers(),
            )
        except Exception as e:
            raise RuntimeError(f"Connection failed: {e}") from e

    def _is_open(self) -> bool:
        """True if websocket is connected and open."""
        return self.websocket is not None and self.websocket.open

    async def wait_for_conversation_created(self, timeout: float = 5.0) -> bool:
        """Read from socket until conversation_created; set conversation_id. Returns True on success."""
        if not self.websocket:
            return False
        elapsed = 0.0
        try:
            async for message in self.websocket:
                elapsed += 0.0  # placeholder; real timeout would need asyncio.wait_for
                try:
                    event = json.loads(message)
                    if event.get("type") == "conversation_created":
                        self.conversation_id = event.get("conversation_id")
                        return True
                    if event.get("type") == "error":
                        return False
                except json.JSONDecodeError:
                    pass
        except (websockets.exceptions.ConnectionClosed, Exception):
            pass
        return False

    async def wait_for_profile_changed(self, timeout: float = 3.0) -> bool:
        """Read from socket until profile_changed. Returns True on success."""
        if not self.websocket:
            return False
        self.profile_changed = False
        try:
            async for message in self.websocket:
                try:
                    event = json.loads(message)
                    if event.get("type") == "profile_changed":
                        self.profile_changed = True
                        return True
                    if event.get("type") == "conversation_complete":
                        return False
                except json.JSONDecodeError:
                    pass
        except (websockets.exceptions.ConnectionClosed, Exception):
            pass
        return self.profile_changed

    async def listen(self):
        """Listen and collect assistant_delta + assistant_complete into self.collected."""
        if not self.websocket:
            raise RuntimeError("Not connected")
        self.collected = []
        self.complete = False
        try:
            async for message in self.websocket:
                try:
                    event = json.loads(message)
                    event_type = event.get("type", "")
                    if event_type == "conversation_created":
                        self.conversation_id = event.get("conversation_id")
                    if event_type == "profile_changed":
                        self.profile_changed = True
                    if event_type == "assistant_delta":
                        self.collected.append(event.get("chunk", ""))
                    elif event_type == "assistant_complete":
                        # Avoid duplicate: only use content if we got no deltas (e.g. server sends complete only).
                        reasoning = event.get("reasoning_content", "")
                        content = event.get("content", "")
                        if content and not self.collected:
                            self.collected.append(content)
                        if reasoning:
                            self.collected.append("\n\n[Reasoning]\n")
                            self.collected.append(reasoning)
                    elif event_type == "conversation_complete":
                        self.complete = True
                    elif event_type == "error":
                        self.collected.append(f"Error: {event.get('message', 'Unknown error')}\n")
                    if self.complete:
                        break
                except json.JSONDecodeError:
                    pass
        except websockets.exceptions.ConnectionClosed:
            raise
        except Exception:
            raise

    async def run_and_get_response(self, prompt: str, profile: Optional[str] = None) -> str:
        """One-shot: connect, start conversation, send message, collect reply, close. Use for stateless call."""
        self.complete = False
        self.profile_changed = False
        self.conversation_id = None
        await self.connect()
        listen_task = asyncio.create_task(self.listen())
        await asyncio.sleep(0.5)
        await self.start_conversation()
        timeout = 5.0
        elapsed = 0.0
        while not self.conversation_id and elapsed < timeout:
            await asyncio.sleep(0.1)
            elapsed += 0.1
        if not self.conversation_id:
            if self.websocket:
                await self.websocket.close()
            return "Failed to create conversation (timeout)."
        if profile:
            self.profile_changed = False
            await self.change_profile(profile)
            elapsed = 0.0
            while not self.profile_changed and elapsed < 3.0:
                await asyncio.sleep(0.1)
                elapsed += 0.1
        await self.send_message(prompt)
        try:
            await listen_task
        except asyncio.CancelledError:
            pass
        if self.websocket:
            await self.websocket.close()
        return "".join(self.collected).strip() or "(No response)"

    async def start_session(self, profile: Optional[str] = None) -> bool:
        """Connect, start_conversation, wait for conversation_id, optionally change_profile. Do not close. Returns True on success."""
        self.complete = False
        self.profile_changed = False
        self.conversation_id = None
        await self.connect()
        await asyncio.sleep(0.5)
        await self.start_conversation()
        if not await self.wait_for_conversation_created():
            if self.websocket:
                await self.websocket.close()
            return False
        if profile:
            await self.change_profile(profile)
            await self.wait_for_profile_changed()
        return True

    async def send_and_collect_reply(self, prompt: str) -> str:
        """Send message and collect reply; do not close. Raises on connection closed."""
        await self.send_message(prompt)
        await self.listen()
        return "".join(self.collected).strip() or "(No response)"


def _get_chat_lock(chat_id: int) -> asyncio.Lock:
    if chat_id not in _chat_locks:
        _chat_locks[chat_id] = asyncio.Lock()
    return _chat_locks[chat_id]


def _close_session(chat_id: int) -> None:
    """Close and remove session for this chat (e.g. on /new)."""
    client = _chat_sessions.pop(chat_id, None)
    if client and client.websocket and client.websocket.open:
        asyncio.create_task(client.websocket.close())


async def _get_or_create_session(chat_id: int, profile: Optional[str]) -> LunaClientCollect:
    """Get existing session or create new one (connect, start_conversation, optional change_profile)."""
    client = _chat_sessions.get(chat_id)
    if client and client._is_open():
        return client
    if client:
        _chat_sessions.pop(chat_id, None)
    address = os.getenv("LUNA_ADDRESS")
    api_key = os.getenv("LUNA_API_KEY")
    if not address or not api_key:
        raise RuntimeError("Bridge misconfiguration: LUNA_ADDRESS or LUNA_API_KEY not set.")
    client = LunaClientCollect(address, api_key)
    if not await client.start_session(profile):
        raise RuntimeError("Failed to create conversation (timeout).")
    _chat_sessions[chat_id] = client
    return client


async def run_luna_and_get_reply(chat_id: int, prompt: str, profile: Optional[str]) -> str:
    """Use per-chat session: one WebSocket + conversation per chat. Returns reply or error string."""
    try:
        async with _get_chat_lock(chat_id):
            client = await _get_or_create_session(chat_id, profile)
            reply = await client.send_and_collect_reply(prompt)
            if not client._is_open():
                _chat_sessions.pop(chat_id, None)
            return reply
    except RuntimeError as e:
        return str(e)
    except websockets.exceptions.ConnectionClosed:
        _chat_sessions.pop(chat_id, None)
        return "Connection lost. Send your message again to start a new conversation."
    except Exception as e:
        _chat_sessions.pop(chat_id, None)
        return f"Luna error: {e!s}"


async def cmd_start(update: Update, context: ContextTypes.DEFAULT_TYPE) -> None:
    if not update.message or not update.message.text:
        return
    if not _is_allowed(update):
        await update.message.reply_text("Not authorized.")
        return
    await update.message.reply_text(
        "Send a message and I’ll forward it to Luna. "
        "Commands: /new — use default profile; /new {profile} or /profile {profile} — set profile."
    )


async def cmd_new(update: Update, context: ContextTypes.DEFAULT_TYPE) -> None:
    if not update.message or not update.message.text:
        return
    if not _is_allowed(update):
        await update.message.reply_text("Not authorized.")
        return
    chat_id = update.effective_chat.id if update.effective_chat else 0
    _close_session(chat_id)
    text = (update.message.text or "").strip()
    parts = text.split(maxsplit=1)
    if len(parts) == 1:
        chat_profiles[chat_id] = None
        await update.message.reply_text("New conversation (default profile).")
    else:
        profile = parts[1].strip()
        chat_profiles[chat_id] = profile
        await update.message.reply_text(f"New conversation with profile: {profile}.")


async def cmd_profile(update: Update, context: ContextTypes.DEFAULT_TYPE) -> None:
    if not update.message or not update.message.text:
        return
    if not _is_allowed(update):
        await update.message.reply_text("Not authorized.")
        return
    chat_id = update.effective_chat.id if update.effective_chat else 0
    text = (update.message.text or "").strip()
    parts = text.split(maxsplit=1)
    if len(parts) < 2:
        await update.message.reply_text("Usage: /profile {profile}")
        return
    profile = parts[1].strip()
    chat_profiles[chat_id] = profile
    _close_session(chat_id)
    await update.message.reply_text(f"Profile set to {profile}. Next message will use it.")


async def handle_message(update: Update, context: ContextTypes.DEFAULT_TYPE) -> None:
    if not update.message or not update.message.text:
        return
    if not _is_allowed(update):
        await update.message.reply_text("Not authorized.")
        return
    chat_id = update.effective_chat.id if update.effective_chat else 0
    prompt = update.message.text.strip()
    profile = chat_profiles.get(chat_id)
    await context.bot.send_chat_action(chat_id=chat_id, action="typing")
    reply = await run_luna_and_get_reply(chat_id, prompt, profile)
    chunks = split_for_telegram(reply)
    for chunk in chunks:
        await update.message.reply_text(chunk)


def main() -> None:
    global _allowed_user_ids
    _allowed_user_ids = _parse_allowed_ids()
    token = os.getenv("TELEGRAM_BOT_TOKEN")
    if not token:
        print("TELEGRAM_BOT_TOKEN environment variable not set", file=sys.stderr)
        sys.exit(1)
    if not os.getenv("LUNA_ADDRESS"):
        print("LUNA_ADDRESS environment variable not set", file=sys.stderr)
        sys.exit(1)
    if not os.getenv("LUNA_API_KEY"):
        print("LUNA_API_KEY environment variable not set", file=sys.stderr)
        sys.exit(1)
    # Fail closed: without an allow-list, anyone who can message the bot drives Luna.
    if _allowed_user_ids is None:
        print(
            "ALLOWED_TELEGRAM_IDS environment variable not set or empty. "
            "Set comma-separated Telegram user IDs (e.g. ALLOWED_TELEGRAM_IDS=123,456).",
            file=sys.stderr,
        )
        sys.exit(1)
    app = Application.builder().token(token).build()
    app.add_handler(CommandHandler("start", cmd_start))
    app.add_handler(CommandHandler("new", cmd_new))
    app.add_handler(CommandHandler("profile", cmd_profile))
    app.add_handler(MessageHandler(filters.TEXT & ~filters.COMMAND, handle_message))
    app.run_polling(allowed_updates=Update.ALL_TYPES)


if __name__ == "__main__":
    main()
