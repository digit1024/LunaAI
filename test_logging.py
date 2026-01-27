#!/usr/bin/env python3
"""
Test script to trigger various error scenarios and verify logging
"""

import asyncio
import json
import websockets
import sys


async def test_error_scenarios():
    """Test various error scenarios to trigger logging"""

    # Connect to WebSocket server
    uri = "ws://localhost:8080"
    headers = {"x-api-key": "test-key", "authorization": "Bearer test-key"}

    try:
        async with websockets.connect(uri, extra_headers=headers) as websocket:
            print("Connected to server")

            # Test 1: Invalid JSON
            print("\n=== Test 1: Invalid JSON ===")
            await websocket.send('{"type": "invalid_json"')

            # Test 2: Invalid command type
            print("\n=== Test 2: Invalid command type ===")
            await websocket.send(
                json.dumps({"type": "unknown_command", "data": "test"})
            )

            # Test 3: Missing required fields
            print("\n=== Test 3: Missing required fields ===")
            await websocket.send(json.dumps({"type": "send_message"}))

            # Test 4: Invalid conversation ID
            print("\n=== Test 4: Invalid conversation ID ===")
            await websocket.send(
                json.dumps(
                    {
                        "type": "send_message",
                        "conversation_id": "invalid-uuid",
                        "content": "test message",
                    }
                )
            )

            # Wait for responses
            for i in range(10):
                try:
                    response = await asyncio.wait_for(websocket.recv(), timeout=1.0)
                    print(f"Response {i + 1}: {response}")
                except asyncio.TimeoutError:
                    break

    except Exception as e:
        print(f"Connection error: {e}")


if __name__ == "__main__":
    asyncio.run(test_error_scenarios())
