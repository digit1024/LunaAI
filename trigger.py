#!/usr/bin/env python3
"""
Luna AI WebSocket Client - Trigger Script

Connects to Luna server via WebSocket, starts a new conversation,
sends a prompt, and prints all messages/tool calls until completion.
"""

import asyncio
import argparse
import json
import os
import sys
from typing import Optional
from urllib.parse import urlparse, urlunparse
import websockets
from websockets.client import WebSocketClientProtocol


class LunaClient:
    def __init__(self, address: str, api_key: str):
        self.address = address
        self.api_key = api_key
        self.websocket: Optional[WebSocketClientProtocol] = None
        self.conversation_id: Optional[str] = None
        self.complete = False
        self.profile_changed = False

    def _get_ws_url(self) -> str:
        """Convert address to WebSocket URL. Server expects path /ws."""
        address = self.address.strip()
        if address.startswith("ws://") or address.startswith("wss://"):
            url = address
        elif address.startswith("http://"):
            url = address.replace("http://", "ws://", 1)
        elif address.startswith("https://"):
            url = address.replace("https://", "wss://", 1)
        else:
            # No scheme: use wss for port 443, else ws
            if ":443" in address or address.endswith(":443"):
                url = f"wss://{address}"
            else:
                url = f"ws://{address}"
        # Ensure path is /ws (server route)
        parsed = urlparse(url)
        if not parsed.path or parsed.path == "/":
            parsed = parsed._replace(path="/ws")
            url = urlunparse(parsed)
        return url

    def _get_headers(self) -> dict:
        """Get WebSocket connection headers."""
        return {
            "x-api-key": self.api_key,
            "authorization": f"Bearer {self.api_key}",
        }

    async def connect(self):
        """Connect to the WebSocket server."""
        url = self._get_ws_url()
        print(f"🔌 Connecting to {url}...", file=sys.stderr)
        
        try:
            self.websocket = await websockets.connect(
                url,
                extra_headers=self._get_headers()
            )
            print("✅ Connected", file=sys.stderr)
        except Exception as e:
            print(f"❌ Connection failed: {e}", file=sys.stderr)
            sys.exit(1)

    async def send_command(self, command: dict):
        """Send a command to the server."""
        if not self.websocket:
            raise RuntimeError("Not connected")
        
        payload = json.dumps(command)
        await self.websocket.send(payload)

    async def start_conversation(self):
        """Start a new conversation."""
        await self.send_command({"type": "start_conversation", "title": None})
        print("📝 Started new conversation", file=sys.stderr)

    async def change_profile(self, profile: str):
        """Change the LLM profile."""
        await self.send_command({"type": "change_profile", "profile": profile})
        print(f"🔄 Changing profile to: {profile}", file=sys.stderr)

    async def list_profiles(self):
        """List available profiles."""
        await self.send_command({"type": "list_profiles"})

    async def send_message(self, content: str):
        """Send a message to the current conversation."""
        command = {
            "type": "send_message",
            "content": content,
            "conversation_id": self.conversation_id,
        }
        await self.send_command(command)
        print(f"💬 Sent message ({len(content)} chars)", file=sys.stderr)

    def _format_event(self, event: dict) -> str:
        """Format an event for display."""
        event_type = event.get("type", "unknown")
        
        if event_type == "health_ok":
            return f"💚 Health OK - Profile: {event.get('profile', 'unknown')}"
        
        elif event_type == "error":
            return f"❌ Error: {event.get('message', 'Unknown error')}"
        
        elif event_type == "conversation_created":
            self.conversation_id = event.get("conversation_id")
            return f"✨ Conversation created: {self.conversation_id}"
        
        elif event_type == "message_accepted":
            return f"✓ Message accepted"
        
        elif event_type == "streaming_started":
            return f"🚀 Streaming started"
        
        elif event_type == "assistant_delta":
            # Print chunk inline (no newline)
            chunk = event.get("chunk", "")
            return chunk
        
        elif event_type == "reasoning_content_delta":
            chunk = event.get("chunk", "")
            return f"🤔 [Reasoning] {chunk}"
        
        elif event_type == "assistant_complete":
            content = event.get("content", "")
            reasoning = event.get("reasoning_content")
            if content:
                result = f"\n\n✅ Assistant response complete:\n{content}"
            else:
                result = f"\n\n✅ Assistant response complete (empty response)"
            if reasoning:
                result += f"\n\n🤔 Reasoning:\n{reasoning}"
            return result
        
        elif event_type == "tool_planned":
            tools = event.get("tools", [])
            result = f"\n🔧 Tools planned ({len(tools)}):\n"
            for tool in tools:
                result += f"  - {tool.get('name', 'unknown')} (id: {tool.get('id', 'unknown')})\n"
            return result
        
        elif event_type == "tool_started":
            name = event.get("name", "unknown")
            tool_call_id = event.get("tool_call_id", "unknown")
            params = event.get("params_json", {})
            return f"\n🔧 Tool started: {name} (id: {tool_call_id})\n  Params: {json.dumps(params, indent=2)}"
        
        elif event_type == "tool_result":
            name = event.get("name", "unknown")
            tool_call_id = event.get("tool_call_id", "unknown")
            result = event.get("result_json", {})
            return f"\n✅ Tool result: {name} (id: {tool_call_id})\n  Result: {json.dumps(result, indent=2)}"
        
        elif event_type == "tool_error":
            name = event.get("name", "unknown")
            tool_call_id = event.get("tool_call_id", "unknown")
            error = event.get("error", "Unknown error")
            return f"\n❌ Tool error: {name} (id: {tool_call_id})\n  Error: {error}"
        
        elif event_type == "profile_changed":
            profile = event.get("profile", "unknown")
            self.profile_changed = True
            return f"✅ Profile changed to: {profile}"
        
        elif event_type == "profiles_list":
            profiles = event.get("profiles", [])
            default = event.get("default_profile", "unknown")
            result = f"\n📋 Available profiles (default: {default}):\n"
            for profile in profiles:
                marker = " (default)" if profile == default else ""
                result += f"  - {profile}{marker}\n"
            return result
        
        elif event_type == "conversation_complete":
            self.complete = True
            return f"\n\n🏁 Conversation complete"
        
        else:
            # Unknown event type - print full JSON
            return f"\n[Event: {event_type}]\n{json.dumps(event, indent=2)}"

    async def listen(self):
        """Listen for events and print them."""
        if not self.websocket:
            raise RuntimeError("Not connected")
        
        last_was_delta = False
        
        try:
            async for message in self.websocket:
                try:
                    event = json.loads(message)
                    event_type = event.get("type", "")
                    
                    # Update conversation_id if we get it
                    if event_type == "conversation_created":
                        self.conversation_id = event.get("conversation_id")
                    
                    formatted = self._format_event(event)
                    
                    # Handle delta events specially (inline printing)
                    if event_type == "assistant_delta":
                        # Print inline without newline
                        print(formatted, end="", flush=True)
                        last_was_delta = True
                    elif event_type == "conversation_complete":
                        if last_was_delta:
                            print()  # Add newline after delta stream
                            last_was_delta = False
                        print(formatted)
                        # self.complete is already set to True in _format_event
                    else:
                        # Print with newline
                        if last_was_delta:
                            print()  # Add newline after delta stream
                            last_was_delta = False
                        print(formatted)
                    
                    if self.complete:
                        break
                        
                except json.JSONDecodeError as e:
                    print(f"\n⚠️ Failed to parse message: {e}", file=sys.stderr)
                    print(f"Raw message: {message}", file=sys.stderr)
                except Exception as e:
                    print(f"\n⚠️ Error processing event: {e}", file=sys.stderr)
        
        except websockets.exceptions.ConnectionClosed:
            print("\n🔌 Connection closed", file=sys.stderr)
        except Exception as e:
            print(f"\n❌ Error in listen loop: {e}", file=sys.stderr)

    async def run(self, prompt: str, profile: Optional[str] = None):
        """Run the full workflow."""
        await self.connect()
        
        # Start listening in background
        listen_task = asyncio.create_task(self.listen())
        
        # Wait a bit for connection to stabilize
        await asyncio.sleep(0.5)
        
        # Start conversation
        await self.start_conversation()
        
        # Wait for conversation to be created (with timeout)
        timeout = 5.0
        elapsed = 0.0
        while not self.conversation_id and elapsed < timeout:
            await asyncio.sleep(0.1)
            elapsed += 0.1
        
        if not self.conversation_id:
            print("❌ Failed to create conversation (timeout)", file=sys.stderr)
            listen_task.cancel()
            if self.websocket:
                await self.websocket.close()
            sys.exit(1)
        
        # Change profile if specified
        if profile:
            self.profile_changed = False
            await self.change_profile(profile)
            # Wait for profile change confirmation (with timeout)
            timeout = 3.0
            elapsed = 0.0
            while not self.profile_changed and elapsed < timeout:
                await asyncio.sleep(0.1)
                elapsed += 0.1
            if not self.profile_changed:
                print("⚠️ Warning: Profile change confirmation not received", file=sys.stderr)
        
        # Send message
        await self.send_message(prompt)
        
        # Wait for completion
        try:
            await listen_task
        except asyncio.CancelledError:
            pass
        
        # Close connection
        if self.websocket:
            await self.websocket.close()


def read_prompt_from_file(filepath: str) -> str:
    """Read prompt from a file."""
    # Resolve path (handle both relative and absolute paths)
    if not os.path.isabs(filepath):
        filepath = os.path.abspath(filepath)
    
    if not os.path.exists(filepath):
        print(f"❌ File not found: {filepath}", file=sys.stderr)
        sys.exit(1)
    
    if not os.path.isfile(filepath):
        print(f"❌ Path is not a file: {filepath}", file=sys.stderr)
        sys.exit(1)
    
    try:
        with open(filepath, "r", encoding="utf-8") as f:
            content = f.read()
            if not content.strip():
                print(f"⚠️ Warning: File {filepath} is empty", file=sys.stderr)
            return content.strip()
    except PermissionError:
        print(f"❌ Permission denied reading file: {filepath}", file=sys.stderr)
        sys.exit(1)
    except Exception as e:
        print(f"❌ Failed to read file {filepath}: {e}", file=sys.stderr)
        sys.exit(1)


def read_multiline_prompt() -> str:
    """Read multiline prompt from stdin."""
    print("Enter your prompt (end with Ctrl+D or empty line + Enter):", file=sys.stderr)
    lines = []
    try:
        while True:
            line = input()
            if not line and lines:  # Empty line after content
                break
            lines.append(line)
    except EOFError:
        pass
    
    return "\n".join(lines).strip()


def main():
    parser = argparse.ArgumentParser(
        description="Luna AI WebSocket Client - Send prompt and receive response",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Environment Variables:
  LUNA_ADDRESS    - Server address (e.g., localhost:8080 or ws://localhost:8080)
  LUNA_API_KEY   - API key for authentication

Examples:
  # From command line argument
  export LUNA_ADDRESS=localhost:8080
  export LUNA_API_KEY=your-key-here
  python trigger.py "What is the weather?"

  # From file
  python trigger.py -f prompt.txt

  # With specific profile
  python trigger.py -p openai "What is the weather?"

  # List available profiles
  python trigger.py --list-profiles

  # Multiline prompt
  python trigger.py
        """
    )
    
    parser.add_argument(
        "prompt",
        nargs="?",
        help="Prompt text (if not provided, will read from stdin or use -f)"
    )
    
    parser.add_argument(
        "-f", "--file",
        help="Read prompt from file"
    )
    
    parser.add_argument(
        "-p", "--profile",
        help="LLM profile to use (e.g., 'openai', 'anthropic', 'ollama')"
    )
    
    parser.add_argument(
        "--list-profiles",
        action="store_true",
        help="List available profiles and exit"
    )
    
    args = parser.parse_args()
    
    # Get address and API key from environment
    address = os.getenv("LUNA_ADDRESS")
    api_key = os.getenv("LUNA_API_KEY")
   

    if not address:
        print("❌ LUNA_ADDRESS environment variable not set", file=sys.stderr)
        sys.exit(1)
    
    if not api_key:
        print("❌ LUNA_API_KEY environment variable not set", file=sys.stderr)
        sys.exit(1)
    
    # Get prompt
    if args.file:
        prompt = read_prompt_from_file(args.file)
        if not prompt:
            print("❌ File is empty or contains only whitespace", file=sys.stderr)
            sys.exit(1)
    elif args.prompt:
        prompt = args.prompt
    else:
        prompt = read_multiline_prompt()
    
    if not prompt:
        print("❌ No prompt provided", file=sys.stderr)
        sys.exit(1)
    
    # Debug: show prompt length (but not content to avoid cluttering)
    print(f"📄 Prompt loaded ({len(prompt)} characters)", file=sys.stderr)
    
    # Run client
    client = LunaClient(address, api_key)
    try:
        # Handle list profiles request
        if args.list_profiles:
            async def list_and_exit():
                await client.connect()
                listen_task = asyncio.create_task(client.listen())
                await asyncio.sleep(0.5)
                await client.list_profiles()
                # Wait a bit for response
                await asyncio.sleep(1.0)
                listen_task.cancel()
                if client.websocket:
                    await client.websocket.close()
            
            asyncio.run(list_and_exit())
        else:
            asyncio.run(client.run(prompt, profile=args.profile))
    except KeyboardInterrupt:
        print("\n\n⚠️ Interrupted by user", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
