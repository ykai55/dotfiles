---
name: aliyun-oss-upload-url
description: Use when a user asks to upload a local file online, publish a file, get an external URL, share a file link, or upload via aliyun CLI/OSS.
---

# Aliyun OSS Upload URL

## Overview

Upload a local file to Aliyun OSS with `aliyun` CLI and return an externally reachable URL. Default bucket is `oss://lllw-qrcodes`. Default object name is a UUID, preserving the original extension when present. Refuse files larger than 50 MiB.

## Workflow

1. Confirm the local file exists and is a regular file.
2. Check size before upload: max `52428800` bytes.
3. Generate the object name from a UUID. Preserve extension if the source basename has one.
4. Verify `aliyun` is available and can access `oss://lllw-qrcodes`.
5. Upload with `aliyun oss cp`.
6. Verify the object with `aliyun oss stat`.
7. For the default `lllw-qrcodes` bucket, build the public URL with the bound custom domain: `https://qrcodes.lllw.cc/<object>`.
8. If the bucket/object is not public, use `aliyun oss sign` for a signed URL or ask before changing ACL.

## Command Pattern

```bash
FILE="/path/to/file.ext"
BUCKET="oss://lllw-qrcodes"
BUCKET_NAME="lllw-qrcodes"
MAX_BYTES=52428800

test -f "$FILE"
SIZE=$(stat -c%s "$FILE")
test "$SIZE" -le "$MAX_BYTES"

EXT=""
BASE=$(basename "$FILE")
case "$BASE" in
  *.*) EXT=".${BASE##*.}" ;;
esac
OBJECT="$(uuidgen)$EXT"

command -v aliyun
aliyun oss stat "$BUCKET"
aliyun oss cp "$FILE" "$BUCKET/$OBJECT" --force
aliyun oss stat "$BUCKET/$OBJECT"
```

Get the public endpoint from `aliyun oss stat "oss://lllw-qrcodes"` only when bucket metadata is needed. For the default bucket, return this public URL format:

```text
https://qrcodes.lllw.cc/<object>
```

Do not return `https://lllw-qrcodes.oss-cn-beijing.aliyuncs.com/<object>` for browser-facing QR code links. The default OSS domain may force `Content-Disposition: attachment`, causing browsers to download instead of display the image.

## Quick Reference

| Need | Command |
| --- | --- |
| Bucket metadata | `aliyun oss stat "oss://lllw-qrcodes"` |
| Upload | `aliyun oss cp "$FILE" "oss://lllw-qrcodes/$OBJECT" --force` |
| Verify object | `aliyun oss stat "oss://lllw-qrcodes/$OBJECT"` |
| Public URL | `https://qrcodes.lllw.cc/$OBJECT` |
| Signed URL fallback | `aliyun oss sign "oss://lllw-qrcodes/$OBJECT"` |

## Common Mistakes

- Do not upload files over 50 MiB. Stop and tell the user the size limit.
- Do not reuse the original filename by default; use a UUID to avoid collisions and accidental disclosure.
- Do not assume `jq`, `ossapi`, or extra tools are installed.
- Do not claim the upload worked until `aliyun oss stat` confirms the object.
- Do not use the default OSS domain for browser-facing QR code links; use `https://qrcodes.lllw.cc/<object>`.
- Do not change bucket or object ACL without explicit user approval.
