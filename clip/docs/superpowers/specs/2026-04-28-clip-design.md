# Clip Design

## Summary

`clip` is a cross-platform clipboard CLI built around a small Rust core and pluggable platform backends. V1 delivers working `macOS` and `Linux` support, keeps `Windows` and `ADB` as explicit extension points, defaults to text operations, and adds opt-in MIME-based read/write for a small set of built-in rich types plus backend-specific custom MIME support.

## Goals

- Provide a single CLI for clipboard read and write across multiple targets.
- Default to safe text mode for common usage.
- Support opt-in non-text clipboard operations through MIME types.
- Fully implement `macOS` and `Linux` in V1.
- Leave `Windows` and `ADB` as first-class backend slots with stubs and tests.
- Keep the architecture open for future remote clipboard targets.
- Ship solid automated coverage, with `macOS` and `Linux` logic reaching at least 80% coverage in the code that can be measured deterministically.

## Non-Goals For V1

- No remote clipboard protocol yet.
- No long-running daemon or background sync.
- No promise that every backend supports arbitrary custom MIME equally well.
- No generalized file-sync semantics beyond clipboard formats such as `text/uri-list`.

## Confirmed Scope

- V1 implementation targets: `macOS`, `Linux/Wayland`, `Linux/X11`.
- `Windows` and `ADB` are present as backend kinds and stub implementations that return explicit `not implemented` errors.
- Linux may depend on system clipboard tools.
- Supported built-in rich types in V1:
  - `text/plain`
  - `text/html`
  - `image/png`
  - `text/uri-list`
- CLI also allows `--type <mime>` for custom MIME values.
- Custom MIME is best-effort per backend; unsupported backends return a clear error instead of silently coercing data.

## Architecture

The project should be a Rust workspace with a small number of focused crates.

- `Cargo.toml`
  - Workspace root and shared dependency definitions.
- `crates/clip-cli`
  - CLI entrypoint.
  - Parses arguments, loads input bytes/text, selects a backend, prints output, and maps domain errors to user-facing messages and exit codes.
- `crates/clip-core`
  - Backend-neutral domain layer.
  - Defines MIME wrappers, request/response types, capability descriptions, backend trait, target selection logic, and shared error types.
- `crates/clip-platform`
  - Concrete platform backends.
  - Contains `macos`, `linux_wayland`, `linux_x11`, `windows_stub`, and `adb_stub` modules.
- `helpers/macos/`
  - Optional thin native helper in Swift or Objective-C if direct Rust integration with `NSPasteboard` is awkward for rich clipboard types.
- `tests/`
  - CLI and integration coverage.
- `testdata/`
  - Sample text, HTML, PNG, and URI-list fixtures.

This separation keeps CLI behavior stable while letting new backends plug into the same contract.

## Core Abstractions

The backend interface should stay synchronous in V1.

Reasoning:

- Local clipboard access on `macOS` and `Linux` is short-lived and blocking.
- The CLI is a one-shot process, so async would add runtime and trait complexity without practical benefit.
- Future remote support can introduce async in a transport-specific layer without forcing all local backends to become async.

Use a request/response shape instead of many special-case methods so the core API stays future-proof for remote transport.

Illustrative interface shape:

```rust
pub trait ClipboardBackend: Send + Sync {
    fn name(&self) -> &'static str;
    fn capabilities(&self) -> BackendCapabilities;
    fn list_types(&self) -> Result<Vec<MimeType>, ClipError>;
    fn read(&self, req: ReadRequest) -> Result<ClipboardBlob, ClipError>;
    fn write(&self, item: ClipboardItem<'_>) -> Result<(), ClipError>;
}
```

Recommended supporting types:

```rust
pub struct ReadRequest {
    pub mime: Option<MimeType>,
    pub prefer_text: bool,
}

pub enum ClipboardBlob {
    Text(String),
    Bytes { mime: MimeType, data: Vec<u8> },
}
```

Other core types should include:

- `TargetKind`
  - `MacOS`
  - `Wayland`
  - `X11`
  - `Windows`
  - `Adb`
- `BackendCapabilities`
  - Whether text read/write is supported.
  - Whether type enumeration is supported.
  - Which built-in MIME types are guaranteed.
  - Whether custom MIME is supported.
- `MimeType`
  - Thin validated wrapper around a string.
- `ClipboardItem`
  - Either text or bytes plus MIME metadata.

## CLI Surface

The CLI should stay small and script-friendly.

Planned commands:

```bash
clip get
clip get --type image/png --output out.png
clip set "hello"
clip set --type text/html --input snippet.html
clip types
clip targets
clip get --target wayland
```

Behavior rules:

- `clip get`
  - Without `--type`, read text and print to stdout.
  - If `--output` is provided, write the text bytes to that file instead of stdout.
- `clip get --type <mime>`
  - Reads the requested type.
  - If the result is binary, require `--output`.
- `clip set <text>`
  - Writes text.
- `clip set --input <path> --type <mime>`
  - Writes raw bytes from a file using the requested MIME.
- `clip set` with stdin and no `--input`
  - Reads from stdin.
- Input sources are mutually exclusive.
  - Positional text, piped stdin, and `--input` may not be combined.
  - If none are provided for `set`, return a usage error.
- `clip types`
  - Prints the current clipboard's readable MIME types, one per line.
- `clip targets`
  - Prints targets detected as available in the current environment.
- `clip targets --all`
  - Includes stubbed future targets such as `windows` and `adb` for discoverability.

Built-in shortcuts may be added as aliases later, but V1 should keep the public interface centered on `--type` rather than grow separate commands per content class.

## Data Flow

### Read Path

1. Parse CLI arguments.
2. Resolve the target backend.
3. Build `ReadRequest`.
4. Backend reads either text or typed bytes.
5. CLI writes text to stdout or bytes to `--output`.

### Write Path

1. Parse CLI arguments.
2. Collect content from argument text, stdin, or `--input`.
3. Normalize content into `ClipboardItem`.
4. Resolve the target backend.
5. Backend writes text or typed bytes.

### Type Discovery Path

1. Parse CLI arguments.
2. Resolve backend.
3. Backend lists types.
4. CLI prints one MIME per line.

## Backend Resolution

Backend resolution should be deterministic and explicit.

Selection rules:

- If `--target` is provided, use it or return a targeted availability error.
- Otherwise on `macOS`, choose the mac backend.
- Otherwise on `Linux`:
  - If `WAYLAND_DISPLAY` is set and `wl-copy` plus `wl-paste` exist, choose Wayland.
  - Else if `DISPLAY` is set and `xclip` exists, choose X11 via `xclip`.
  - Else if `DISPLAY` is set and only `xsel` exists, choose X11 via `xsel` with reduced capability.
  - Else return an actionable error listing the missing tools or environment conditions.
- `Windows` and `ADB` are never auto-selected in V1.

This keeps detection predictable while still supporting mixed desktop setups.

## Platform Design

### macOS Backend

Implementation target:

- Prefer a native implementation against `NSPasteboard`.
- If Rust bindings become awkward for rich type fidelity, use a thin native helper in Swift or Objective-C and invoke it from Rust.

Responsibilities:

- Read and write `text/plain`.
- Read and write `text/html`.
- Read and write `image/png`.
- Read and write `text/uri-list` when supported by pasteboard type mapping.
- Enumerate current available pasteboard types and map them back to MIME values where practical.

Custom MIME behavior:

- Support only where there is a clean pasteboard type mapping.
- If a custom MIME value cannot be represented safely, return an explicit unsupported-type error.

### Linux Wayland Backend

Implementation target:

- Shell out to `wl-copy` and `wl-paste`.

Responsibilities:

- `wl-copy --type <mime>` for typed writes.
- `wl-paste --type <mime>` for typed reads.
- `wl-paste --list-types` for type enumeration.

Wayland is the strongest Linux backend for custom MIME in V1 and should be treated as the reference Linux implementation.

### Linux X11 Backend

Implementation target:

- Prefer `xclip`.
- Fall back to `xsel` for text-only behavior where necessary.

Responsibilities:

- Support text read and write reliably.
- Support built-in rich MIME types where the selected tool can express them.
- Expose reduced capability when only `xsel` is available.

Custom MIME behavior:

- Best effort when using `xclip` with typed targets.
- Unsupported for `xsel` unless the exact operation can be proven reliable.

The backend capability report must reflect these differences so the CLI can produce honest error messages.

### Windows And ADB Stubs

Responsibilities:

- Implement the same trait.
- Return `not implemented` errors.
- Advertise no active capabilities.
- Participate in selection and tests so adding real support later does not require redesigning the core interfaces.

## Error Model

Errors should separate user mistakes from platform failures.

Recommended categories:

- `ConfigError`
  - Invalid CLI combinations.
  - Multiple competing input sources for `set`.
  - Unknown built-in alias.
  - Missing `--output` for binary reads.
- `BackendUnavailable`
  - Requested target not supported on this machine.
  - Required system command missing.
  - Stub backend selected in V1.
- `ClipboardError`
  - Requested MIME not present.
  - Clipboard command failure.
  - Native helper failure.
  - Encoding or decoding mismatch.

CLI presentation rules:

- Successful text reads print raw text to stdout.
- Successful binary reads require `--output` and write no extra text to stdout.
- Successful writes are silent.
- Failures print a single concise message to stderr and return a non-zero exit code.
- `types` and `targets` print one item per line for easy shell composition.

## Testing Strategy

Testing should focus on deterministic behavior first, not on fragile real-desktop automation.

### Unit Tests

Primary coverage areas:

- CLI argument parsing.
- Input source validation.
- MIME parsing and built-in alias resolution.
- Backend selection rules.
- Error-to-exit-code mapping.
- Capability-driven command behavior.

These tests should use fake backends and fake environment providers rather than touching a real clipboard.

### Platform Adapter Tests

Linux adapters:

- Inject fake `PATH` entries with test scripts that emulate `wl-copy`, `wl-paste`, `xclip`, and `xsel`.
- Verify command arguments, stdin payloads, stdout payloads, and failure mapping.

macOS adapter:

- Mock the command runner or helper invocation boundary.
- Verify MIME mapping and error translation.
- Keep true native integration tests minimal and focused on a small happy path.

### Integration Tests

Cover the real CLI surface for:

- `clip get`
- `clip set`
- `clip types`
- `clip targets`
- `--type`
- `--input`
- `--output`
- `--target`

Linux integration strategy:

- Run deterministic command-level integration tests in Docker.
- Use fake clipboard command shims in the container rather than depend on a live graphical session.

macOS integration strategy:

- Run a narrow set of host-only tests for basic text and one rich type path.

Coverage target:

- Reach at least 80% coverage on `macOS` and `Linux` logic that can be measured reliably.
- Prioritize `clip-core`, `clip-cli`, and command-adapter behavior over brittle GUI-session automation.

## Extensibility For Remote Clipboard

The design keeps local backends synchronous but preserves a future path for remote targets:

- Core requests and responses are serializable concepts.
- A future remote backend can translate `ReadRequest` and `ClipboardItem` over HTTP, RPC, or another transport.
- Async can be introduced in a dedicated remote transport crate later without changing the local backend trait.

This avoids premature async complexity in V1 while keeping the model ready for a network boundary.

## Implementation Priorities

Suggested build order:

1. Create the Rust workspace and core types.
2. Build the CLI and fake backend coverage.
3. Implement Wayland backend.
4. Implement X11 backend with `xclip` first and `xsel` fallback behavior second.
5. Implement macOS backend.
6. Add stubs for `windows` and `adb`.
7. Finish integration tests, Docker flow, and user documentation.

## Open Decisions Resolved

- Backend interface remains synchronous in V1.
- Built-in rich MIME support is cross-platform; custom MIME is backend-specific best effort.
- Linux may rely on system commands.
- X11 capability is allowed to degrade when only `xsel` is available.
- `Windows` and `ADB` are architectural placeholders, not shipping backends in V1.
