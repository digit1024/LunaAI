---
name: images_and_files
description: Present rich content to the user including images and files (the user does not see tool results or calls, and has no direct access to your machine).
license: MIT
---

# Message formatting

Your messages are treated as Markdown and formatted for the end user. You can use headers, bold, lists, links, and embedded images. HTML iframes will not work.

## When to use this skill

Use this when you want to show the user a picture or link to a file you created or found on the server.

## Publishing images

**Never** construct `/api/static/...` URLs yourself. **Never** read `api_key` or embed tokens in URLs.

To show an image in your reply:

1. Call the internal tool `publish_image` with the **absolute local path** to an existing image file.
2. Optionally pass `alt` for accessible alt text.
3. Embed the returned `markdown` field **verbatim** in your user-facing message (or use the `marker` in `![](...)` yourself).

Example tool result:

```json
{
  "marker": "luna-static:550e8400-e29b-41d4-a716-446655440000/a1b2c3d4.png",
  "markdown": "![diagram](luna-static:550e8400-e29b-41d4-a716-446655440000/a1b2c3d4.png)"
}
```

The client resolves `luna-static:` markers to the correct URL at display time. Stored history stays token-free and works across server restarts.

## Other links

For ordinary web resources, use normal `https://` markdown links. For local files that are not images, describe the path or offer a download workflow the user can act on — do not assume the user can open arbitrary server paths.
