# Profile Change Fix - Conversation Profile Update

## Problem
When changing profile from mobile app:
- The session profile was updated correctly
- But the conversation's stored profile in the database was NOT updated
- This caused the conversation to continue using the old profile
- "No context is recognized" because the new profile's prompts weren't being used

## Root Cause
The server updates the session profile when `changeProfile` is received, but:
1. The server doesn't track which conversation is currently active
2. The conversation's stored profile in the database wasn't being updated
3. When loading the conversation again, it would restore the old profile from the database

## Solution
Update the conversation's stored profile in the database when sending a message, if it differs from the current session profile. This ensures:
- When you change the profile and send a message, the conversation's profile gets updated
- The conversation will use the new profile going forward
- Loading the conversation will restore the updated profile

## Implementation

### 1. Added Method to Update Conversation Profile

**SQLite Storage** (`src/storage/sqlite_storage_simple.rs`):
```rust
pub fn update_profile(&self, conversation_id: &str, profile_name: Option<&str>) -> SqliteResult<bool>
```

**Storage Wrapper** (`src/storage/storage_wrapper.rs`):
```rust
pub fn update_conversation_profile(&self, id: &Uuid, profile_name: Option<&str>) -> SqliteResult<bool>
```

### 2. Update Conversation Profile When Sending Message

**Server Handler** (`src/server/handlers.rs`):
- When sending a message to an existing conversation:
  - Check if conversation's stored profile differs from session profile
  - If different, update the conversation's stored profile to match session profile
  - This ensures the conversation uses the current session profile going forward

### 3. Set Profile on Load for Old Conversations

**Server Handler** (`src/server/handlers.rs`):
- When loading a conversation that has no profile stored (NULL):
  - Set the conversation's profile to the current session profile
  - This handles old conversations created before profile support

## Flow After Fix

### Changing Profile
1. User changes profile in mobile app dropdown
2. Mobile app sends `changeProfile(new_profile)` command
3. Server updates session profile and rebuilds LLM client
4. Server sends `ProfileChanged` event
5. Mobile app updates UI to show new profile

### Sending Message After Profile Change
1. User sends a message to existing conversation
2. Server checks: conversation profile vs session profile
3. If different, server updates conversation's stored profile in DB
4. Message is processed using the new session profile (with correct prompts/context)
5. Going forward, conversation uses the new profile

### Loading Conversation After Profile Change
1. User loads conversation
2. Server checks conversation's stored profile
3. If conversation has a stored profile, server restores it to session
4. Conversation uses its stored profile for subsequent messages

## Testing

To verify the fix works:
1. Load an existing conversation (or create new one)
2. Change profile using the dropdown in mobile app
3. Send a message in the conversation
4. Check the database - conversation's `profile_name` should be updated
5. Reload the conversation - it should use the new profile
6. Verify the new profile's prompts/context are being used

## Notes

- The conversation profile is updated when sending a message, not immediately when changing the profile
- This is because the server doesn't track which conversation is active
- An alternative would be to add `conversation_id` to the `changeProfile` command, but that requires protocol changes
- Current solution ensures the profile is updated as soon as the user continues the conversation


