# OpenCode provider generator

`bin/opencode-provider-gen` discovers an OpenAI-compatible API's model IDs and
installs a provider into this tracked OpenCode configuration. It uses only the
Python 3 standard library.

## Integrated workflow

Put the fixed provider settings in `~/.config/opencode/.env`:

```dotenv
OPENAI_BASE_URL=https://api.example.com/v1
OPENAI_API_KEY=secret-token
```

Then choose the OpenCode provider name:

```bash
bin/opencode-provider-gen --name my-api
```

The command automatically reads `~/.config/opencode/.env`. If the calling
process already contains `OPENAI_BASE_URL` or `OPENAI_API_KEY`, that process
value takes precedence over the file. Other entries in the file are ignored by
the generator and loader.

The command performs three steps:

1. Reads `GET $OPENAI_BASE_URL/models` with Bearer authentication.
2. Creates or updates `opencode/providers/my-api.json` with all returned model
   IDs and fixed `{env:OPENAI_BASE_URL}` / `{env:OPENAI_API_KEY}` references,
   never their resolved values.
3. Ensures `opencode/opencode.json` contains `"./provider-loader.ts"` in its
   `plugin` array.

`opencode/provider-loader.ts` loads the same `.env` file, resolves the fixed
references, and merges every sorted `providers/*.json` entry into
`config.provider`. After generation, restart OpenCode; no `OPENCODE_CONFIG`
wrapper or exported provider variables are required.

Use a unique `--name` rather than a built-in provider ID such as `openai`. The
name is both the provider ID and UI label. It must contain only letters, digits,
`.`, `_`, and `-`, and becomes the provider JSON filename.

### Select models

The default installs all discovered model IDs:

```bash
# Inspect IDs only; do not change files.
bin/opencode-provider-gen --name my-api --list

# Discover and retain exact IDs or case-sensitive shell globs.
bin/opencode-provider-gen --name my-api --include 'org/*' 'chat-*'

# Select numbered models interactively; Enter selects all.
bin/opencode-provider-gen --name my-api --select

# Skip /models and install exact IDs offline.
bin/opencode-provider-gen --name my-api --models model-a org/model-b
```

Each `--include` pattern must match. `--models` accepts literal IDs, not globs or
comma-separated lists. These modes and `--list` are mutually exclusive.

For an unauthenticated local API, keep the fixed host name and omit the key:

```dotenv
OPENAI_BASE_URL=http://127.0.0.1:1234/v1
```

```bash
bin/opencode-provider-gen --name local-ai --no-auth
```

The same fixed variables apply to every generated provider. Therefore only one
host/token pair can be active in this setup at a time; `--name` changes the
OpenCode provider identity and output filename, not the credential names.

The `.env` file is ignored by Git. The tracked provider JSON can still expose
model or deployment IDs, so review those IDs before committing.

## `.env` syntax

Supported lines are:

```dotenv
OPENAI_BASE_URL=https://api.example.com/v1
OPENAI_API_KEY="a quoted token"
export OPENAI_API_KEY='a quoted token'
```

Blank lines and lines beginning with `#` are ignored. An inline comment is
supported after whitespace. Variable expansion, command substitution and
multiline values are intentionally not evaluated. A malformed non-comment line
fails the command instead of being silently ignored.

## Export-only modes

Inspect valid JSON without installing it:

```bash
bin/opencode-provider-gen --name my-api --models model-a --stdout
```

Create a standalone overlay without changing this OpenCode configuration:

```bash
bin/opencode-provider-gen --name my-api --models model-a \
  -o /path/to/my-api.json
OPENCODE_CONFIG=/path/to/my-api.json opencode
```

`-o` refuses to overwrite an existing path. In integrated mode, only
`opencode/providers/<name>.json` owned by the same provider may be updated; a
malformed, foreign, symlink, or non-file target is refused.

## Loader behavior

- Provider files must be strict UTF-8 JSON with exactly one `provider` entry.
- The provider key must match the filename: `my-api.json` contains `my-api`.
- Generated providers override providers with the same ID already present in
  the main configuration. Avoid duplicate IDs.
- A malformed provider or `.env` file makes the loader fail visibly instead of
  silently using a partial configuration.
- Removing a provider requires intentionally deleting its JSON file and
  restarting OpenCode; the generator does not delete providers.

OpenCode has no native JSON `include` field. `{file:...}` substitutes escaped
string content and cannot insert an object. The registered config-hook plugin is
the integration layer that preserves separate provider files.

## Network and compatibility boundaries

- `OPENAI_BASE_URL` is the full API base URL, including `/v1` when required. The
  script appends only `/models`; do not use a completion or models endpoint.
- Redirects are refused. HTTPS verification remains enabled. Authenticated,
  non-loopback HTTP is refused unless `--allow-http` is explicit.
- `--timeout` defaults to 30 seconds per socket operation. Responses above
  8 MiB and known pagination indicators are refused.
- Only OpenAI-compatible Chat Completions providers are generated, using
  `@ai-sdk/openai-compatible`. Responses API providers, custom auth headers,
  Azure deployment semantics, and nonstandard model lists need manual config.
- `/models` can include embedding/image models. Listing does not prove Chat
  Completions, tool-call, or coding-agent support. Use selection when needed.
- Context/output limits, modalities, prices and capabilities are not inferred.

## Verification

```bash
python3 -m unittest discover -s bin/tests -p 'test_opencode_provider_gen.py' -v
python3 -m unittest discover -s bin/tests -p 'test_*.py'
```

The focused tests execute the real CLI against an isolated loopback HTTP server,
temporary home directories and temporary OpenCode directories, using only fake
credentials.

## References

- [OpenCode configuration](https://opencode.ai/docs/config/)
- [OpenCode providers](https://opencode.ai/docs/providers/)
- [OpenCode plugins](https://opencode.ai/docs/plugins/)
