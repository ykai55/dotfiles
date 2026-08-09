# Management API design

## Scope

Add a private management interface that a trusted operator can use to manage
client identities, client tokens, and subdomain policies without restarting the
rproxy server. The first version keeps one server domain; policies constrain the
single-label subdomain below that domain.

The management interface does not manage tunnels, TCP port permissions, server
configuration, or multiple root domains.

## Deployment interface

Managed mode uses a SQLite database and a dedicated listener:

```text
rproxy server \
  --domain example.com \
  --auth-db /var/lib/rproxy/auth.db \
  --management-listen 127.0.0.1:7001 \
  --management-token-file /run/secrets/rproxy-management-token
```

The management listener defaults to loopback and is separate from the control
WebSocket listener. A reverse proxy may expose it over HTTPS to the trusted
operator. The management credential is read from a permission-restricted file,
is compared in constant time, and is never accepted as a client token.

`--token`, `--config`, and `--auth-db` are mutually exclusive authentication
sources. Legacy and static-config modes do not start the management listener.

## Resource model

### Client identity

```json
{
  "id": "build-agent",
  "enabled": true,
  "subdomain_policy": {
    "rules": ["preview-*", "docs"]
  },
  "created_at": "2026-08-09T10:00:00Z",
  "updated_at": "2026-08-09T10:00:00Z"
}
```

Identity IDs are immutable, unique, non-empty operator-facing names. Disabling
or deleting an identity immediately closes its active control connections and
releases its tunnels.

Subdomain rules are either an exact label (`docs`), a prefix wildcard
(`preview-*`), or the unrestricted wildcard (`*`). Matching is ASCII
case-insensitive after the same normalization used by routing. Patterns cannot
contain dots, multiple wildcards, or a wildcard outside the final position.
When the policy is restricted and the client omits `subdomain`, registration is
rejected; this avoids surprising allocation outside the declared policy.

Changing a policy immediately closes tunnels that no longer satisfy it. New
registrations always use the latest committed policy.

### Client token

```json
{
  "id": "01J...",
  "client_identity_id": "build-agent",
  "label": "production",
  "created_at": "2026-08-09T10:00:00Z",
  "expires_at": null,
  "last_used_at": null
}
```

A client identity can hold multiple tokens for zero-downtime rotation. Token
creation returns the plaintext exactly once. List and get operations return
metadata only. There is no operation to recover or update a token secret;
rotation means create a replacement, deploy it, then revoke the old token.

Revoking a token immediately closes control connections authenticated with that
specific token. Other tokens for the same identity remain valid.

## HTTP interface

Every request requires:

```http
Authorization: Bearer <management-credential>
Content-Type: application/json
```

Endpoints:

```text
POST   /v1/client-identities
GET    /v1/client-identities
GET    /v1/client-identities/{identity_id}
PATCH  /v1/client-identities/{identity_id}
DELETE /v1/client-identities/{identity_id}

POST   /v1/client-identities/{identity_id}/tokens
GET    /v1/client-identities/{identity_id}/tokens
GET    /v1/client-identities/{identity_id}/tokens/{token_id}
DELETE /v1/client-identities/{identity_id}/tokens/{token_id}
```

Create identity request:

```json
{
  "id": "build-agent",
  "subdomain_policy": { "rules": ["preview-*", "docs"] }
}
```

Patch identity request:

```json
{
  "enabled": false,
  "subdomain_policy": { "rules": ["docs"] }
}
```

Create token request and response:

```json
{ "label": "production", "expires_at": null }
```

```json
{
  "token": {
    "id": "01J...",
    "client_identity_id": "build-agent",
    "label": "production",
    "created_at": "2026-08-09T10:00:00Z",
    "expires_at": null,
    "last_used_at": null
  },
  "secret": "rpt_<random-base64url>"
}
```

Successful creates return `201`, reads and patches return `200`, and deletes
return `204`. Errors use a stable JSON shape:

```json
{
  "error": {
    "code": "subdomain_policy_invalid",
    "message": "rule must be an exact label, prefix wildcard, or *"
  }
}
```

Expected status codes are `400` for malformed input, `401` for management
authentication failure, `404` for missing resources, and `409` for identity ID
conflicts. Authentication failures do not reveal whether a client token,
identity, or management credential exists.

## Persistence

SQLite is the source of truth in managed mode. A minimal schema is:

```sql
CREATE TABLE client_identities (
  id TEXT PRIMARY KEY,
  enabled INTEGER NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE subdomain_rules (
  client_identity_id TEXT NOT NULL REFERENCES client_identities(id) ON DELETE CASCADE,
  rule TEXT NOT NULL,
  PRIMARY KEY (client_identity_id, rule)
);

CREATE TABLE client_tokens (
  id TEXT PRIMARY KEY,
  client_identity_id TEXT NOT NULL REFERENCES client_identities(id) ON DELETE CASCADE,
  token_hash BLOB NOT NULL UNIQUE,
  label TEXT,
  created_at TEXT NOT NULL,
  expires_at TEXT,
  last_used_at TEXT
);

CREATE TABLE management_audit_log (
  id INTEGER PRIMARY KEY,
  occurred_at TEXT NOT NULL,
  request_id TEXT NOT NULL,
  action TEXT NOT NULL,
  target_type TEXT NOT NULL,
  target_id TEXT NOT NULL
);
```

Tokens contain at least 256 bits of randomness. Only a SHA-256 digest is stored;
high-entropy generated tokens do not need a password hash. Database migrations
run transactionally at startup. SQLite foreign keys and WAL mode are enabled.
Every mutation and its audit entry commit in one transaction.

## Internal module seams

Keep HTTP, persistence, and live-session behavior behind three narrow
interfaces:

```text
CredentialStore
  authenticate(secret) -> AuthenticatedCredential | AuthenticationFailed
  create_identity(input) -> ClientIdentity
  update_identity(id, patch) -> ClientIdentity
  delete_identity(id)
  create_token(identity_id, input) -> CreatedToken
  revoke_token(identity_id, token_id) -> RevokedCredential
  list/get operations

AuthorizationPolicy
  allows_subdomain(identity, requested_subdomain) -> Allowed | Denied

SessionRegistry
  revoke_identity(identity_id)
  revoke_credential(token_id)
  enforce_policy(identity_id, policy)
```

`CredentialStore` owns SQLite transactions, token generation and hashing, and
domain validation. The control WebSocket asks it to authenticate and receives
the token ID, identity ID, and current policy; callers never inspect database
rows directly. `SessionRegistry` indexes active sessions by identity ID and
token ID so management mutations can take effect immediately.

The management HTTP handlers remain thin adapters: parse and authenticate the
request, call these interfaces, and translate results into HTTP responses.

## Mutation sequence

For mutations that affect live sessions:

1. Commit the database transaction and audit event.
2. Notify `SessionRegistry` using the committed identity or token ID.
3. Return success after the registry has applied cancellation to its current
   in-process sessions.

SQLite remains authoritative. If process termination occurs between steps one
and two, startup begins with no active sessions, so the committed state still
wins. This avoids a distributed transaction between persistence and memory.

## Security constraints

- Bind management to loopback by default; require explicit configuration to
  bind a non-loopback address.
- Require HTTPS at the reverse proxy when traffic leaves the host.
- Never place management endpoints under `/_rproxy` or the public HTTP tunnel
  listener.
- Limit request body size and request rate at the reverse proxy and in axum.
- Redact `Authorization`, client token secrets, and token hashes from logs and
  errors.
- Return a generated request ID and write it to the audit log.
- Use restrictive permissions for the database and management token file.
- Do not permit callers to choose token plaintext; always generate it server
  side.

## Delivery slices

1. Introduce the credential and policy interfaces with in-memory adapters and
   enforce subdomain policies during registration.
2. Add the SQLite adapter, migrations, generated hashed tokens, and startup
   managed mode.
3. Add the private management listener and identity/token CRUD handlers.
4. Index active sessions by identity and token, then implement immediate
   revocation and policy enforcement.
5. Add audit logging, operational metrics, backup documentation, and an
   end-to-end test through the management and control listeners.
