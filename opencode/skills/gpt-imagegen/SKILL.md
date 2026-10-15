---
name: gpt-imagegen
description: Generate or edit raster images through OpenCode's gpt_imagegen tool using the existing ChatGPT/Codex login. Use for requests to generate images, edit photos, replace text in an image, create illustrations, or derive visual variants, including 生图、改图、图片改字. Supports pasted/uploaded chat attachments directly and local PNG, JPEG, and WebP files. Prefer native editing for existing SVG or code-based graphics.
---

# GPT Image Generation

Use the `gpt_imagegen` tool supplied by the matching OpenCode plugin. It reuses the active OpenAI ChatGPT/Codex OAuth connection; no additional API key is required.

## Workflow

1. Identify whether the user wants generation or editing. For a pasted/uploaded chat image, pass its displayed attachment name (such as `image-1.png`) in `attachments`: the plugin reads the actual image bytes from the current session. No local path or clipboard access is needed. For a local file, read it first and pass its path in `images`. Read the clipboard only if the user authorizes it, following the paste skill.
2. Write a concise prompt. For editing, name the exact change and what must remain unchanged. Quote replacement text verbatim. Label each input's role when using multiple images: edit target, style reference, or compositing input.
3. Call `gpt_imagegen` with:
   - `prompt`: requested content or edit instructions.
   - `images`: local input paths, omitted for text-to-image. PNG, JPEG, or WebP, at most 20 MiB each.
   - `attachments`: names of chat image attachments, using the newest user message containing each name. At most four inputs across `images` and `attachments`. If the user means an older image with a reused name, ask them to re-upload it with a unique name. Only request a local path if attachment lookup actually fails; a chat preview alone is not evidence that the bytes are unavailable.
   - `outputDirectory`: optional; defaults to `output/gpt-imagegen` under the session directory. Use the task's artifact directory when appropriate.
   - `model`: normally omit. This selects the orchestrating Codex model, not the image model. Default: `gpt-6-astra`.
4. Read the saved PNG and check the requested text, composition, and preserved details. If a correction is needed, make one targeted follow-up using the output as the next edit target.
5. Return the image and its saved path. Distinguish successful generation from merely receiving HTTP 200. State any visible mismatch honestly.

The tool saves uniquely named files and preserves source images. Do not copy credentials into scripts, prompts, or logs. If authentication fails, ask the user to check their OpenAI login in OpenCode. If the tool is absent, check that the `gpt-imagegen` plugin is active rather than substituting an unavailable built-in image tool.

## Edit example

```json
{
  "prompt": "Edit the supplied sticker. Replace only SU7 with YU7. Match the existing white lettering, size, spacing, and blue background. Preserve all Chinese text, the Xiaomi logo, layout, and other pixels as closely as possible.",
  "attachments": ["image-1.png"]
}
```

## Direct command

`/gpt-imagegen` accepts a plain prompt or a JSON object using the same fields. Prefer the tool during agent work so cancellation follows the tool invocation.

## Interface boundary

This plugin uses the experimental Codex Responses route with the `image_generation` tool. Availability depends on the account and service. It does not claim a particular underlying image-model version. Ordinary OpenAI Images API permissions are separate from Codex OAuth permissions.
