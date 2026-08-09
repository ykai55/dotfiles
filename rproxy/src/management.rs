use crate::auth::{
    normalize_identity_id, ClientIdentity, ClientToken, ConfiguredCredentials, CreateIdentity,
    CreateToken, CreatedToken, CredentialStore, StoreError, UpdateIdentity,
};
use crate::server::{AdmissionGate, ServerState};
use axum::extract::rejection::JsonRejection;
use axum::extract::{DefaultBodyLimit, Path, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{Duration, Instant};
use uuid::Uuid;

const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(60);

#[derive(Clone)]
struct ManagementState {
    catalog: ManagementCatalog,
    server: ServerState,
    admission_gate: AdmissionGate,
    token_digest: [u8; 32],
    rate_limiter: RateLimiter,
}

pub fn router(
    store: CredentialStore,
    configured: ConfiguredCredentials,
    server: ServerState,
    admission_gate: AdmissionGate,
    token: String,
    requests_per_minute: u32,
    body_limit_bytes: usize,
) -> Router {
    let state = ManagementState {
        catalog: ManagementCatalog { store, configured },
        server,
        admission_gate,
        token_digest: Sha256::digest(token.as_bytes()).into(),
        rate_limiter: RateLimiter::new(requests_per_minute),
    };
    Router::new()
        .route(
            "/v1/client-identities",
            post(create_identity).get(list_identities),
        )
        .route(
            "/v1/client-identities/:identity_id",
            get(get_identity)
                .patch(update_identity)
                .delete(delete_identity),
        )
        .route(
            "/v1/client-identities/:identity_id/tokens",
            post(create_token).get(list_tokens),
        )
        .route(
            "/v1/client-identities/:identity_id/tokens/:token_id",
            get(get_token).delete(delete_token),
        )
        .fallback(not_found)
        .method_not_allowed_fallback(method_not_allowed)
        .layer(DefaultBodyLimit::max(body_limit_bytes))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            authenticate_request,
        ))
        .with_state(state)
}

#[derive(Clone)]
struct ManagementCatalog {
    store: CredentialStore,
    configured: ConfiguredCredentials,
}

impl ManagementCatalog {
    async fn create_identity(
        &self,
        mut input: CreateIdentity,
        request_id: String,
    ) -> Result<ClientIdentity, ManagementError> {
        input.id = normalize_identity_id(&input.id)?;
        if self.configured.contains_identity(&input.id) {
            return Err(ManagementError::Store(StoreError::Conflict));
        }
        Ok(self.store.create_identity(input, request_id).await?)
    }

    async fn list_identities(&self) -> Result<Vec<ClientIdentity>, ManagementError> {
        let mut identities = self.configured.list_identities();
        identities.extend(self.store.list_identities().await?);
        identities.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(identities)
    }

    async fn get_identity(&self, identity_id: &str) -> Result<ClientIdentity, ManagementError> {
        if let Some(identity) = self.configured.get_identity(identity_id) {
            return Ok(identity);
        }
        Ok(self.store.get_identity(identity_id).await?)
    }

    async fn update_identity(
        &self,
        identity_id: &str,
        input: UpdateIdentity,
        request_id: String,
    ) -> Result<ClientIdentity, ManagementError> {
        self.reject_config_identity(identity_id)?;
        Ok(self
            .store
            .update_identity(identity_id, input, request_id)
            .await?)
    }

    async fn delete_identity(
        &self,
        identity_id: &str,
        request_id: String,
    ) -> Result<(), ManagementError> {
        self.reject_config_identity(identity_id)?;
        Ok(self.store.delete_identity(identity_id, request_id).await?)
    }

    async fn create_token(
        &self,
        identity_id: &str,
        input: CreateToken,
        request_id: String,
    ) -> Result<CreatedToken, ManagementError> {
        self.reject_config_identity(identity_id)?;
        Ok(self
            .store
            .create_token(
                identity_id,
                input,
                self.configured.token_hashes().copied().collect(),
                request_id,
            )
            .await?)
    }

    async fn list_tokens(&self, identity_id: &str) -> Result<Vec<ClientToken>, ManagementError> {
        if let Some(tokens) = self.configured.list_tokens(identity_id) {
            return Ok(tokens);
        }
        Ok(self.store.list_tokens(identity_id).await?)
    }

    async fn get_token(
        &self,
        identity_id: &str,
        token_id: &str,
    ) -> Result<ClientToken, ManagementError> {
        if let Some(token) = self.configured.get_token(identity_id, token_id) {
            return Ok(token);
        }
        Ok(self.store.get_token(identity_id, token_id).await?)
    }

    async fn delete_token(
        &self,
        identity_id: &str,
        token_id: &str,
        request_id: String,
    ) -> Result<(), ManagementError> {
        self.reject_config_identity(identity_id)?;
        Ok(self
            .store
            .delete_token(identity_id, token_id, request_id)
            .await?)
    }

    fn reject_config_identity(&self, identity_id: &str) -> Result<(), ManagementError> {
        if self.configured.contains_identity(identity_id) {
            Err(ManagementError::ManagedByConfig)
        } else {
            Ok(())
        }
    }
}

async fn not_found() -> ManagementError {
    ManagementError::RouteNotFound
}

async fn method_not_allowed() -> ManagementError {
    ManagementError::MethodNotAllowed
}

#[derive(Clone)]
struct RequestId(String);

async fn authenticate_request(
    State(state): State<ManagementState>,
    mut request: Request,
    next: Next,
) -> Response {
    let request_id = Uuid::new_v4().to_string();
    if !authorized(request.headers(), &state.token_digest) {
        let mut response = ManagementError::Unauthorized.into_response();
        add_request_id(&mut response, &request_id);
        return response;
    }
    if !state.rate_limiter.allow().await {
        let mut response = ManagementError::RateLimited.into_response();
        add_request_id(&mut response, &request_id);
        return response;
    }
    request
        .extensions_mut()
        .insert(RequestId(request_id.clone()));
    let mut response = next.run(request).await;
    add_request_id(&mut response, &request_id);
    response
}

#[derive(Clone)]
struct RateLimiter {
    window: Arc<Mutex<RateLimitWindow>>,
}

struct RateLimitWindow {
    started_at: Instant,
    requests: u32,
    max_requests: u32,
}

impl RateLimiter {
    fn new(max_requests: u32) -> Self {
        Self {
            window: Arc::new(Mutex::new(RateLimitWindow {
                started_at: Instant::now(),
                requests: 0,
                max_requests,
            })),
        }
    }

    async fn allow(&self) -> bool {
        let mut window = self.window.lock().await;
        if window.started_at.elapsed() >= RATE_LIMIT_WINDOW {
            window.started_at = Instant::now();
            window.requests = 0;
        }
        if window.requests >= window.max_requests {
            return false;
        }
        window.requests += 1;
        true
    }
}

fn add_request_id(response: &mut Response, request_id: &str) {
    response.headers_mut().insert(
        "x-request-id",
        HeaderValue::from_str(request_id).expect("UUID is a valid header value"),
    );
}

async fn create_identity(
    State(state): State<ManagementState>,
    Extension(request_id): Extension<RequestId>,
    input: Result<Json<CreateIdentity>, JsonRejection>,
) -> Result<(StatusCode, Json<ClientIdentity>), ManagementError> {
    let Json(input) = input.map_err(ManagementError::InvalidJson)?;
    let _admission = state.admission_gate.write().await;
    let identity = state.catalog.create_identity(input, request_id.0).await?;
    state
        .server
        .apply_identity(
            &identity.id,
            identity.enabled,
            identity.subdomain_policy.clone(),
        )
        .await;
    Ok((StatusCode::CREATED, Json(identity)))
}

async fn list_identities(
    State(state): State<ManagementState>,
) -> Result<Json<Vec<ClientIdentity>>, ManagementError> {
    Ok(Json(state.catalog.list_identities().await?))
}

async fn get_identity(
    State(state): State<ManagementState>,
    Path(identity_id): Path<String>,
) -> Result<Json<ClientIdentity>, ManagementError> {
    Ok(Json(state.catalog.get_identity(&identity_id).await?))
}

async fn update_identity(
    State(state): State<ManagementState>,
    Path(identity_id): Path<String>,
    Extension(request_id): Extension<RequestId>,
    input: Result<Json<UpdateIdentity>, JsonRejection>,
) -> Result<Json<ClientIdentity>, ManagementError> {
    let Json(input) = input.map_err(ManagementError::InvalidJson)?;
    let _admission = state.admission_gate.write().await;
    let identity = state
        .catalog
        .update_identity(&identity_id, input, request_id.0)
        .await?;
    state
        .server
        .apply_identity(
            &identity.id,
            identity.enabled,
            identity.subdomain_policy.clone(),
        )
        .await;
    Ok(Json(identity))
}

async fn delete_identity(
    State(state): State<ManagementState>,
    Path(identity_id): Path<String>,
    Extension(request_id): Extension<RequestId>,
) -> Result<StatusCode, ManagementError> {
    let _admission = state.admission_gate.write().await;
    state
        .catalog
        .delete_identity(&identity_id, request_id.0)
        .await?;
    state.server.delete_identity(&identity_id).await;
    Ok(StatusCode::NO_CONTENT)
}

async fn create_token(
    State(state): State<ManagementState>,
    Path(identity_id): Path<String>,
    Extension(request_id): Extension<RequestId>,
    input: Result<Json<CreateToken>, JsonRejection>,
) -> Result<(StatusCode, Json<CreatedToken>), ManagementError> {
    let Json(input) = input.map_err(ManagementError::InvalidJson)?;
    let _admission = state.admission_gate.write().await;
    let token = state
        .catalog
        .create_token(&identity_id, input, request_id.0)
        .await?;
    Ok((StatusCode::CREATED, Json(token)))
}

async fn list_tokens(
    State(state): State<ManagementState>,
    Path(identity_id): Path<String>,
) -> Result<Json<Vec<ClientToken>>, ManagementError> {
    Ok(Json(state.catalog.list_tokens(&identity_id).await?))
}

async fn get_token(
    State(state): State<ManagementState>,
    Path((identity_id, token_id)): Path<(String, String)>,
) -> Result<Json<ClientToken>, ManagementError> {
    Ok(Json(
        state.catalog.get_token(&identity_id, &token_id).await?,
    ))
}

async fn delete_token(
    State(state): State<ManagementState>,
    Path((identity_id, token_id)): Path<(String, String)>,
    Extension(request_id): Extension<RequestId>,
) -> Result<StatusCode, ManagementError> {
    let _admission = state.admission_gate.write().await;
    state
        .catalog
        .delete_token(&identity_id, &token_id, request_id.0)
        .await?;
    state.server.revoke_token(&token_id).await;
    Ok(StatusCode::NO_CONTENT)
}

fn authorized(headers: &HeaderMap, expected: &[u8; 32]) -> bool {
    let actual = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or("");
    let actual: [u8; 32] = Sha256::digest(actual.as_bytes()).into();
    constant_time_eq(&actual, expected)
}

fn constant_time_eq(left: &[u8; 32], right: &[u8; 32]) -> bool {
    let mut difference = 0_u8;
    for index in 0..left.len() {
        difference |= left[index] ^ right[index];
    }
    difference == 0
}

#[derive(Debug)]
enum ManagementError {
    Unauthorized,
    RateLimited,
    RouteNotFound,
    MethodNotAllowed,
    ManagedByConfig,
    InvalidJson(JsonRejection),
    Store(StoreError),
}

impl From<StoreError> for ManagementError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

#[derive(Serialize)]
struct ErrorResponse {
    error: ErrorBody,
}

#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    message: String,
}

impl IntoResponse for ManagementError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "authentication failed".to_string(),
            ),
            Self::RateLimited => (
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                "management request rate limit exceeded".to_string(),
            ),
            Self::RouteNotFound => (
                StatusCode::NOT_FOUND,
                "not_found",
                "resource not found".to_string(),
            ),
            Self::MethodNotAllowed => (
                StatusCode::METHOD_NOT_ALLOWED,
                "method_not_allowed",
                "method not allowed".to_string(),
            ),
            Self::ManagedByConfig => (
                StatusCode::CONFLICT,
                "managed_by_config",
                "resource is managed by the server configuration".to_string(),
            ),
            Self::InvalidJson(error) => (
                StatusCode::BAD_REQUEST,
                "invalid_request",
                error.body_text(),
            ),
            Self::Store(StoreError::Invalid(message)) => {
                (StatusCode::BAD_REQUEST, "invalid_request", message)
            }
            Self::Store(StoreError::NotFound) => (
                StatusCode::NOT_FOUND,
                "not_found",
                "resource not found".to_string(),
            ),
            Self::Store(StoreError::Conflict) => (
                StatusCode::CONFLICT,
                "conflict",
                "resource already exists".to_string(),
            ),
            Self::Store(StoreError::Internal(error)) => {
                tracing::error!("management request failed: {error}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "internal server error".to_string(),
                )
            }
        };
        (
            status,
            Json(ErrorResponse {
                error: ErrorBody { code, message },
            }),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_management_tokens() {
        let secret: [u8; 32] = Sha256::digest(b"secret").into();
        let other: [u8; 32] = Sha256::digest(b"other").into();
        assert!(constant_time_eq(&secret, &secret));
        assert!(!constant_time_eq(&secret, &other));
    }
}
