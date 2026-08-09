# Unified server configuration design

## Goal

Make `rproxy server --config` the complete server configuration interface. The
file configures listeners, public HTTP metadata, static client identities,
SQLite-backed API identities, and the management interface.

There is one supported file shape. No format version or compatibility path is
provided. A full configuration is authoritative and cannot be mixed with other
server CLI options.

## Configuration format

```toml
[server]
domain = "example.com"
control_listen = "127.0.0.1:7000"
http_listen = "0.0.0.0:8080"
tcp_port_range = "20000-30000"
http_public_scheme = "https"
http_public_port = 443

[authentication]
database = "/var/lib/rproxy/auth.db"

[management]
listen = "127.0.0.1:7001"
token_file = "/run/secrets/rproxy-management-token"
requests_per_minute = 120
body_limit_bytes = 16384

[[clients]]
id = "build-agent"
enabled = true
subdomains = ["preview-*", "docs"]

Subdomain rules are single-label rules. They may be exact labels, `*`, or one
wildcard inside a single label, for example `preview-*`, `*-dev`, or
`api-*-dev`. Multi-level rules such as `*.dev` are not supported.

[[clients.tokens]]
name = "production"
label = "production"
token = "rpt_configured_secret"

[[clients.tokens]]
name = "rotation"
label = "next production token"
token = "rpt_next_configured_secret"
expires_at = "2027-01-01T00:00:00Z"

[[clients]]
id = "personal-nas"
enabled = true
subdomains = ["nas"]

[[clients.tokens]]
name = "primary"
token = "rpt_another_configured_secret"
```

`http_public_port` is optional. `management.requests_per_minute` and
`management.body_limit_bytes` default to the current built-in limits. The
management bearer secret is never accepted inline; `token_file` is mandatory.

Because the file contains client token plaintext, Unix deployments require mode
`0600`. rproxy rejects a configuration with group or world permission bits. The
management token file is independently restricted to `0600`.

## CLI behavior

```text
rproxy server --config /etc/rproxy/server.toml
```

No other server option may accompany `--config`. CLI values do not override
file values. The existing clients-only config shape is removed; `--config`
always means the complete file above.

`--token` may remain as a separate development convenience only if it continues
to configure all required non-auth server values explicitly. It is not combined
with the full config file.

## Two management domains

Configured records and API-created records have separate storage and ownership:

| Source | Storage | Mutation owner |
| --- | --- | --- |
| Config | Parsed immutable in-memory snapshot | Configuration file and restart |
| API | SQLite | Management API |

Config identities and tokens are never inserted, updated, or represented by
placeholder rows in SQLite. SQLite contains only resources created through the
management API.

At startup, rproxy loads both sources and validates the combined credential
namespace before binding listeners. Startup fails if:

- A config Identity ID already exists in SQLite.
- A config token plaintext hashes to the same digest as an SQLite token.
- Config contains duplicate Identity IDs, token names, or token plaintext.
- Any subdomain rule, timestamp, listener, port range, or required path is
  invalid.

There is no shadowing, automatic deletion, conversion, or synchronization. A
conflict must be resolved explicitly by changing the file or deleting the API
resource before adding the config resource.

## Runtime authentication

The authentication module presents one lookup interface over two adapters:

```text
CredentialProvider
  authenticate(secret) -> AuthenticatedCredential | AuthenticationFailed

ConfiguredCredentials
  immutable in-memory adapter

CredentialStore
  SQLite adapter for API-managed records
```

Startup proves token digests are unique across both adapters, so authentication
cannot return ambiguous results. Runtime lookup may check the immutable config
map first and SQLite second, but this ordering is an optimization rather than a
conflict-resolution rule.

`AuthenticatedCredential` includes its management domain:

```text
identity_id
token_id
managed_by = config | api
subdomain_policy
```

Config token IDs are stable in-memory IDs derived from client ID and token
`name`. API token IDs remain generated UUIDs. Data WebSockets continue to bind
to the exact authenticated token ID.

Reload is not part of the first implementation. Editing the file requires a
server restart. API mutations remain live and immediately affect API-managed
sessions.

## Management API behavior

List and get operations expose a merged view and identify ownership:

```json
{
  "id": "build-agent",
  "managed_by": "config"
}
```

Mutation rules are strict:

- Config identities and tokens are readable but immutable through the API.
- PATCH or DELETE of a config resource returns `409 managed_by_config`.
- The API cannot create a token under a config identity.
- Creating an API identity with a config Identity ID returns `409 conflict`.
- Creating an API token whose secret collides with a config token returns
  `409 conflict`.
- API identities and API tokens retain normal CRUD behavior.

The management module receives the immutable config snapshot alongside
`CredentialStore`. It performs cross-source uniqueness checks before SQLite
writes. SQLite constraints remain responsible for races between concurrent API
requests; the shared admission gate coordinates API mutations with control
registration.

No API operation writes config records into SQLite.

## SQLite schema

No source columns, source keys, config placeholders, or reconciliation metadata
are needed. The existing SQLite schema remains dedicated to API-managed
identities and tokens.

The only schema changes should be those independently required by API behavior.
In particular, the implementation must not add:

```text
source
source_key
managed_by_config
```

`managed_by` is computed by the merged management interface, not persisted for
config resources.

## Internal interfaces

Keep parsing, combined validation, runtime authentication, and management
queries behind explicit seams:

```text
ServerConfigFile::parse(text) -> ServerConfigFile
ServerConfigFile::validate() -> ValidatedServerConfig

ConfiguredCredentials::new(clients) -> ConfiguredCredentials
ConfiguredCredentials::authenticate(secret) -> Option<AuthenticatedCredential>
ConfiguredCredentials::contains_identity(id) -> bool
ConfiguredCredentials::contains_token_digest(digest) -> bool
ConfiguredCredentials::list/get operations

CredentialStore
  SQLite authentication and CRUD for API resources only

ManagementCatalog
  merged list/get operations
  config ownership checks before API mutations
```

`ValidatedServerConfig` is the only type consumed by server startup. It contains
parsed socket addresses, validated port ranges, normalized subdomain policies,
validated token metadata, and resolved paths.

`ManagementCatalog` earns its seam by hiding merged reads and ownership checks
from HTTP handlers. Handlers should not independently query config and SQLite or
reimplement conflict rules.

## Startup sequence

1. Read the config and enforce file permissions.
2. Deserialize and validate the complete file.
3. Build the immutable `ConfiguredCredentials` snapshot.
4. Open and migrate the API-only SQLite database.
5. Check Identity ID and token digest uniqueness across config and SQLite.
6. Read and validate the management token file.
7. Build the combined authentication provider and management catalog.
8. Bind control, HTTP, and management listeners.

Any failure before step eight exits without exposing a partially configured
server. Config data is never written during startup.

## Removal behavior

Removing an Identity or token from the config requires a restart. On the next
startup it simply does not exist in the immutable snapshot. No SQLite operation
is performed.

If an API resource was previously blocked by a config conflict, the server would
not have started; therefore removal cannot unexpectedly reveal a shadowed API
resource. After the conflict is removed, normal startup validation determines
the resulting combined namespace.

## Test coverage

Implementation should cover:

- Parsing the complete configuration without a version field.
- Rejecting `--config` combined with any other server option.
- Rejecting the removed clients-only format.
- Rejecting insecure config and management-token file permissions.
- Verifying config identities and tokens never appear in SQLite tables.
- Authenticating both config and API tokens through one running server.
- Returning merged list/get results with `managed_by`.
- Returning `409 managed_by_config` for config mutations.
- Rejecting API token creation under a config identity.
- Rejecting API Identity ID collisions with config.
- Rejecting API token-secret collisions with config.
- Failing startup on pre-existing cross-source conflicts.
- Removing a config token without changing SQLite.
- Starting no listeners when parsing, validation, or cross-source checks fail.
