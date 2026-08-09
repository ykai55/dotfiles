use crate::alloc::normalize_subdomain_label;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, SecondsFormat, Utc};
use rand::rngs::OsRng;
use rand::RngCore;
use rusqlite::{params, Connection, ErrorCode, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedCredential {
    pub identity_id: String,
    pub token_id: String,
    pub subdomain_policy: SubdomainPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubdomainPolicy {
    pub rules: Vec<String>,
}

impl SubdomainPolicy {
    pub fn unrestricted() -> Self {
        Self {
            rules: vec!["*".into()],
        }
    }

    pub fn new(rules: Vec<String>) -> Result<Self, StoreError> {
        if rules.is_empty() {
            return Err(StoreError::Invalid(
                "subdomain policy must contain at least one rule".into(),
            ));
        }
        let mut normalized = Vec::with_capacity(rules.len());
        for rule in rules {
            let rule = rule.to_ascii_lowercase();
            if rule == "*" {
                normalized.push(rule);
                continue;
            }
            if let Some(prefix) = rule.strip_suffix('*') {
                if !prefix.ends_with('-') || prefix[..prefix.len() - 1].contains('*') {
                    return Err(StoreError::Invalid(format!(
                        "invalid subdomain rule {rule:?}"
                    )));
                }
                normalize_subdomain_label(prefix.trim_end_matches('-'))?;
                normalized.push(rule);
                continue;
            }
            if rule.contains('*') {
                return Err(StoreError::Invalid(format!(
                    "invalid subdomain rule {rule:?}"
                )));
            }
            normalized.push(normalize_subdomain_label(&rule)?);
        }
        normalized.sort();
        normalized.dedup();
        Ok(Self { rules: normalized })
    }

    pub fn allows(&self, requested: Option<&str>) -> bool {
        let Some(requested) = requested else {
            return self.rules.iter().any(|rule| rule == "*");
        };
        let Ok(requested) = normalize_subdomain_label(requested) else {
            return false;
        };
        self.rules.iter().any(|rule| {
            rule == "*"
                || rule == &requested
                || rule
                    .strip_suffix('*')
                    .is_some_and(|prefix| requested.starts_with(prefix))
        })
    }
}

impl From<crate::alloc::AllocError> for StoreError {
    fn from(error: crate::alloc::AllocError) -> Self {
        Self::Invalid(error.to_string())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ClientIdentity {
    pub id: String,
    pub enabled: bool,
    pub subdomain_policy: SubdomainPolicy,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateIdentity {
    pub id: String,
    pub subdomain_policy: SubdomainPolicy,
}

#[derive(Debug, Deserialize)]
pub struct UpdateIdentity {
    pub enabled: Option<bool>,
    pub subdomain_policy: Option<SubdomainPolicy>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClientToken {
    pub id: String,
    pub client_identity_id: String,
    pub label: Option<String>,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub last_used_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateToken {
    pub label: Option<String>,
    pub expires_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreatedToken {
    pub token: ClientToken,
    pub secret: String,
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("{0}")]
    Invalid(String),
    #[error("resource not found")]
    NotFound,
    #[error("resource already exists")]
    Conflict,
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

#[derive(Clone)]
pub struct CredentialStore {
    connection: Arc<Mutex<Connection>>,
}

impl CredentialStore {
    pub fn open(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref().to_path_buf();
        prepare_database_file(&path)?;
        let mut connection = Connection::open(&path)?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version > 1 {
            anyhow::bail!("authentication database schema version {version} is unsupported");
        }
        if version == 0 {
            let transaction = connection.transaction()?;
            transaction.execute_batch(
                r#"
CREATE TABLE IF NOT EXISTS client_identities (
    id TEXT PRIMARY KEY,
    enabled INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS subdomain_rules (
    client_identity_id TEXT NOT NULL REFERENCES client_identities(id) ON DELETE CASCADE,
    rule TEXT NOT NULL,
    PRIMARY KEY (client_identity_id, rule)
);
CREATE TABLE IF NOT EXISTS client_tokens (
    id TEXT PRIMARY KEY,
    client_identity_id TEXT NOT NULL REFERENCES client_identities(id) ON DELETE CASCADE,
    token_hash BLOB NOT NULL UNIQUE,
    label TEXT,
    created_at TEXT NOT NULL,
    expires_at TEXT,
    last_used_at TEXT
);
CREATE TABLE IF NOT EXISTS management_audit_log (
    id INTEGER PRIMARY KEY,
    occurred_at TEXT NOT NULL,
    request_id TEXT NOT NULL,
    action TEXT NOT NULL,
    target_type TEXT NOT NULL,
    target_id TEXT NOT NULL
);
PRAGMA user_version = 1;
"#,
            )?;
            transaction.commit()?;
        }
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    async fn run<T, F>(&self, operation: F) -> Result<T, StoreError>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> Result<T, StoreError> + Send + 'static,
    {
        let connection = self.connection.clone();
        tokio::task::spawn_blocking(move || {
            let mut connection = connection
                .lock()
                .map_err(|_| StoreError::Internal(anyhow::anyhow!("SQLite lock poisoned")))?;
            operation(&mut connection)
        })
        .await
        .map_err(|error| StoreError::Internal(error.into()))?
    }

    pub async fn authenticate(
        &self,
        secret: &str,
    ) -> Result<Option<AuthenticatedCredential>, StoreError> {
        let hash = token_hash(secret).to_vec();
        self.run(move |connection| {
            let timestamp = now();
            let row = connection
                .query_row(
                    r#"
SELECT t.id, t.client_identity_id
FROM client_tokens t
JOIN client_identities i ON i.id = t.client_identity_id
WHERE t.token_hash = ?1 AND i.enabled = 1
  AND (t.expires_at IS NULL OR t.expires_at > ?2)
"#,
                    params![hash, timestamp],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()
                .map_err(internal)?;
            let Some((token_id, identity_id)) = row else {
                return Ok(None);
            };
            connection
                .execute(
                    "UPDATE client_tokens SET last_used_at = ?1 WHERE id = ?2",
                    params![now(), token_id],
                )
                .map_err(internal)?;
            let policy = load_policy(connection, &identity_id)?;
            Ok(Some(AuthenticatedCredential {
                identity_id,
                token_id,
                subdomain_policy: policy,
            }))
        })
        .await
    }

    pub async fn create_identity(
        &self,
        input: CreateIdentity,
        request_id: String,
    ) -> Result<ClientIdentity, StoreError> {
        let id = validate_identity_id(&input.id)?;
        let policy = SubdomainPolicy::new(input.subdomain_policy.rules)?;
        self.run(move |connection| {
            let timestamp = now();
            let transaction = connection.transaction().map_err(internal)?;
            if let Err(error) = transaction.execute(
                "INSERT INTO client_identities (id, enabled, created_at, updated_at) VALUES (?1, 1, ?2, ?2)",
                params![id, timestamp],
            ) {
                return Err(sqlite_write_error(error));
            }
            replace_policy(&transaction, &id, &policy)?;
            audit(&transaction, &request_id, "create", "client_identity", &id)?;
            transaction.commit().map_err(internal)?;
            Ok(ClientIdentity {
                id,
                enabled: true,
                subdomain_policy: policy,
                created_at: timestamp.clone(),
                updated_at: timestamp,
            })
        })
        .await
    }

    pub async fn list_identities(&self) -> Result<Vec<ClientIdentity>, StoreError> {
        self.run(move |connection| {
            let rows = {
                let mut statement = connection
                    .prepare("SELECT id, enabled, created_at, updated_at FROM client_identities ORDER BY id")
                    .map_err(internal)?;
                let rows = statement
                    .query_map([], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, bool>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                        ))
                    })
                    .map_err(internal)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(internal)?;
                rows
            };
            rows.into_iter()
                .map(|(id, enabled, created_at, updated_at)| {
                    Ok(ClientIdentity {
                        subdomain_policy: load_policy(connection, &id)?,
                        id,
                        enabled,
                        created_at,
                        updated_at,
                    })
                })
                .collect()
        })
        .await
    }

    pub async fn get_identity(&self, id: &str) -> Result<ClientIdentity, StoreError> {
        let id = id.to_string();
        self.run(move |connection| load_identity(connection, &id))
            .await
    }

    pub async fn update_identity(
        &self,
        id: &str,
        input: UpdateIdentity,
        request_id: String,
    ) -> Result<ClientIdentity, StoreError> {
        let id = id.to_string();
        let policy = input
            .subdomain_policy
            .map(|policy| SubdomainPolicy::new(policy.rules))
            .transpose()?;
        self.run(move |connection| {
            let transaction = connection.transaction().map_err(internal)?;
            let current = load_identity(&transaction, &id)?;
            let enabled = input.enabled.unwrap_or(current.enabled);
            let policy = policy.unwrap_or(current.subdomain_policy);
            let timestamp = now();
            transaction
                .execute(
                    "UPDATE client_identities SET enabled = ?1, updated_at = ?2 WHERE id = ?3",
                    params![enabled, timestamp, id],
                )
                .map_err(internal)?;
            replace_policy(&transaction, &id, &policy)?;
            audit(&transaction, &request_id, "update", "client_identity", &id)?;
            transaction.commit().map_err(internal)?;
            Ok(ClientIdentity {
                id,
                enabled,
                subdomain_policy: policy,
                created_at: current.created_at,
                updated_at: timestamp,
            })
        })
        .await
    }

    pub async fn delete_identity(&self, id: &str, request_id: String) -> Result<(), StoreError> {
        let id = id.to_string();
        self.run(move |connection| {
            let transaction = connection.transaction().map_err(internal)?;
            let changed = transaction
                .execute("DELETE FROM client_identities WHERE id = ?1", [&id])
                .map_err(internal)?;
            if changed == 0 {
                return Err(StoreError::NotFound);
            }
            audit(&transaction, &request_id, "delete", "client_identity", &id)?;
            transaction.commit().map_err(internal)?;
            Ok(())
        })
        .await
    }

    pub async fn create_token(
        &self,
        identity_id: &str,
        input: CreateToken,
        request_id: String,
    ) -> Result<CreatedToken, StoreError> {
        let identity_id = identity_id.to_string();
        let expires_at = input
            .expires_at
            .map(|value| {
                parse_timestamp(&value)
                    .map(|timestamp| timestamp.to_rfc3339_opts(SecondsFormat::Secs, true))
            })
            .transpose()?;
        self.run(move |connection| {
            load_identity(connection, &identity_id)?;
            let mut random = [0_u8; 32];
            OsRng.fill_bytes(&mut random);
            let secret = format!("rpt_{}", URL_SAFE_NO_PAD.encode(random));
            let token = ClientToken {
                id: Uuid::new_v4().to_string(),
                client_identity_id: identity_id.clone(),
                label: input.label,
                created_at: now(),
                expires_at,
                last_used_at: None,
            };
            let transaction = connection.transaction().map_err(internal)?;
            transaction
                .execute(
                    "INSERT INTO client_tokens (id, client_identity_id, token_hash, label, created_at, expires_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![token.id, token.client_identity_id, token_hash(&secret).to_vec(), token.label, token.created_at, token.expires_at],
                )
                .map_err(sqlite_write_error)?;
            audit(&transaction, &request_id, "create", "client_token", &token.id)?;
            transaction.commit().map_err(internal)?;
            Ok(CreatedToken { token, secret })
        })
        .await
    }

    pub async fn list_tokens(&self, identity_id: &str) -> Result<Vec<ClientToken>, StoreError> {
        let identity_id = identity_id.to_string();
        self.run(move |connection| {
            load_identity(connection, &identity_id)?;
            let mut statement = connection
                .prepare("SELECT id, label, created_at, expires_at, last_used_at FROM client_tokens WHERE client_identity_id = ?1 ORDER BY created_at, id")
                .map_err(internal)?;
            let tokens = statement
                .query_map([&identity_id], |row| {
                    Ok(ClientToken {
                        id: row.get(0)?,
                        client_identity_id: identity_id.clone(),
                        label: row.get(1)?,
                        created_at: row.get(2)?,
                        expires_at: row.get(3)?,
                        last_used_at: row.get(4)?,
                    })
                })
                .map_err(internal)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(internal)?;
            Ok(tokens)
        })
        .await
    }

    pub async fn get_token(
        &self,
        identity_id: &str,
        token_id: &str,
    ) -> Result<ClientToken, StoreError> {
        let identity_id = identity_id.to_string();
        let token_id = token_id.to_string();
        self.run(move |connection| load_token(connection, &identity_id, &token_id))
            .await
    }

    pub async fn delete_token(
        &self,
        identity_id: &str,
        token_id: &str,
        request_id: String,
    ) -> Result<(), StoreError> {
        let identity_id = identity_id.to_string();
        let token_id = token_id.to_string();
        self.run(move |connection| {
            let transaction = connection.transaction().map_err(internal)?;
            let changed = transaction
                .execute(
                    "DELETE FROM client_tokens WHERE id = ?1 AND client_identity_id = ?2",
                    params![token_id, identity_id],
                )
                .map_err(internal)?;
            if changed == 0 {
                return Err(StoreError::NotFound);
            }
            audit(
                &transaction,
                &request_id,
                "delete",
                "client_token",
                &token_id,
            )?;
            transaction.commit().map_err(internal)?;
            Ok(())
        })
        .await
    }
}

fn prepare_database_file(path: &PathBuf) -> anyhow::Result<()> {
    if path == Path::new(":memory:") {
        return Ok(());
    }
    #[cfg(unix)]
    {
        use std::fs::{self, OpenOptions};
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

        OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .mode(0o600)
            .open(path)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn validate_identity_id(id: &str) -> Result<String, StoreError> {
    let id = id.trim();
    if id.is_empty() || id.len() > 128 || !id.is_ascii() {
        return Err(StoreError::Invalid(
            "identity id must be 1-128 ASCII characters".into(),
        ));
    }
    Ok(id.to_string())
}

fn token_hash(secret: &str) -> [u8; 32] {
    Sha256::digest(secret.as_bytes()).into()
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn parse_timestamp(value: &str) -> Result<DateTime<Utc>, StoreError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| StoreError::Invalid("expires_at must be an RFC 3339 timestamp".into()))
}

fn load_policy(connection: &Connection, identity_id: &str) -> Result<SubdomainPolicy, StoreError> {
    let mut statement = connection
        .prepare("SELECT rule FROM subdomain_rules WHERE client_identity_id = ?1 ORDER BY rule")
        .map_err(internal)?;
    let rules = statement
        .query_map([identity_id], |row| row.get(0))
        .map_err(internal)?
        .collect::<Result<Vec<String>, _>>()
        .map_err(internal)?;
    SubdomainPolicy::new(rules)
}

fn load_identity(connection: &Connection, id: &str) -> Result<ClientIdentity, StoreError> {
    let identity = connection
        .query_row(
            "SELECT enabled, created_at, updated_at FROM client_identities WHERE id = ?1",
            [id],
            |row| {
                Ok((
                    row.get::<_, bool>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()
        .map_err(internal)?;
    let Some((enabled, created_at, updated_at)) = identity else {
        return Err(StoreError::NotFound);
    };
    Ok(ClientIdentity {
        id: id.to_string(),
        enabled,
        subdomain_policy: load_policy(connection, id)?,
        created_at,
        updated_at,
    })
}

fn load_token(
    connection: &Connection,
    identity_id: &str,
    token_id: &str,
) -> Result<ClientToken, StoreError> {
    connection
        .query_row(
            "SELECT label, created_at, expires_at, last_used_at FROM client_tokens WHERE id = ?1 AND client_identity_id = ?2",
            params![token_id, identity_id],
            |row| {
                Ok(ClientToken {
                    id: token_id.to_string(),
                    client_identity_id: identity_id.to_string(),
                    label: row.get(0)?,
                    created_at: row.get(1)?,
                    expires_at: row.get(2)?,
                    last_used_at: row.get(3)?,
                })
            },
        )
        .optional()
        .map_err(internal)?
        .ok_or(StoreError::NotFound)
}

fn replace_policy(
    transaction: &Transaction<'_>,
    identity_id: &str,
    policy: &SubdomainPolicy,
) -> Result<(), StoreError> {
    transaction
        .execute(
            "DELETE FROM subdomain_rules WHERE client_identity_id = ?1",
            [identity_id],
        )
        .map_err(internal)?;
    for rule in &policy.rules {
        transaction
            .execute(
                "INSERT INTO subdomain_rules (client_identity_id, rule) VALUES (?1, ?2)",
                params![identity_id, rule],
            )
            .map_err(internal)?;
    }
    Ok(())
}

fn audit(
    transaction: &Transaction<'_>,
    request_id: &str,
    action: &str,
    target_type: &str,
    target_id: &str,
) -> Result<(), StoreError> {
    transaction
        .execute(
            "INSERT INTO management_audit_log (occurred_at, request_id, action, target_type, target_id) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![now(), request_id, action, target_type, target_id],
        )
        .map_err(internal)?;
    Ok(())
}

fn internal(error: rusqlite::Error) -> StoreError {
    StoreError::Internal(error.into())
}

fn sqlite_write_error(error: rusqlite::Error) -> StoreError {
    match &error {
        rusqlite::Error::SqliteFailure(failure, _)
            if failure.code == ErrorCode::ConstraintViolation =>
        {
            StoreError::Conflict
        }
        _ => internal(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_and_matches_subdomain_policy() {
        let policy = SubdomainPolicy::new(vec!["Docs".into(), "preview-*".into()]).unwrap();
        assert!(policy.allows(Some("docs")));
        assert!(policy.allows(Some("preview-123")));
        assert!(!policy.allows(Some("other")));
        assert!(!policy.allows(None));
    }

    #[tokio::test]
    async fn stores_only_token_hash_and_authenticates_secret() {
        let store = CredentialStore::open(":memory:").unwrap();
        store
            .create_identity(
                CreateIdentity {
                    id: "agent".into(),
                    subdomain_policy: SubdomainPolicy::unrestricted(),
                },
                "request-1".into(),
            )
            .await
            .unwrap();
        let created = store
            .create_token(
                "agent",
                CreateToken {
                    label: None,
                    expires_at: None,
                },
                "request-2".into(),
            )
            .await
            .unwrap();

        let authenticated = store.authenticate(&created.secret).await.unwrap().unwrap();
        assert_eq!(authenticated.identity_id, "agent");
        assert_eq!(authenticated.token_id, created.token.id);
        assert!(store.authenticate("wrong").await.unwrap().is_none());
    }
}
