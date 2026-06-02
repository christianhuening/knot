//! knot server library — exports `router()` for tests + state for main.

use std::sync::Arc;

use axum::{
    Router,
    extract::{Path, State, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
};
use knot_auth::{Hasher, Throttle};
use knot_config::Config;
use knot_docs::AclCache;
use knot_storage::{
    DocStore, GrantStore, PgDocStore, PgGrantStore, PgSessionStore, PgUserStore, PgWorkspaceStore,
    Pool, SessionStore, UserStore, WorkspaceStore,
};
use uuid::Uuid;

pub mod auth;
pub mod http_error;
pub mod protocol;
pub mod room;
pub mod routes;

use auth::SessionDeps;

#[derive(Clone)]
pub struct AppState {
    pub pool: Option<Pool>,
    pub users: Option<Arc<dyn UserStore>>,
    pub workspaces: Option<Arc<dyn WorkspaceStore>>,
    pub sessions: Option<Arc<dyn SessionStore>>,
    pub docs: Option<Arc<dyn DocStore>>,
    pub grants: Option<Arc<dyn GrantStore>>,
    pub acl: Option<Arc<AclCache>>,
    pub rooms_v2: Option<Arc<knot_crdt::Rooms>>,
    pub bus: Option<Arc<dyn knot_crdt::Bus>>,
    pub hasher: Arc<Hasher>,
    pub throttle: Arc<Throttle>,
    pub session_key: Vec<u8>,
    pub base_url: String,
    pub oidc_enabled: bool,
    pub oidc: Option<Arc<knot_auth::oidc::OidcClient>>,
    pub config: Arc<Config>,
}

impl AppState {
    pub fn in_memory() -> Self {
        Self {
            pool: None,
            users: None,
            workspaces: None,
            sessions: None,
            docs: None,
            grants: None,
            acl: None,
            rooms_v2: None,
            bus: None,
            hasher: Arc::new(Hasher::new()),
            throttle: Arc::new(Throttle::new()),
            session_key: Vec::new(),
            base_url: "http://localhost:3000".into(),
            oidc_enabled: false,
            oidc: None,
            config: Arc::new(Config::default()),
        }
    }

    /// Constructor used by `main` + integration tests when a real Postgres
    /// pool is available. Wires every storage trait to the corresponding
    /// `Pg*` impl so callers don't have to assemble them by hand. Caller is
    /// still responsible for setting `session_key`, `base_url`, and
    /// `oidc_enabled` from configuration.
    pub fn with_pool(pool: Pool) -> Self {
        let users: Arc<dyn UserStore> = Arc::new(PgUserStore::new(pool.clone()));
        let workspaces: Arc<dyn WorkspaceStore> = Arc::new(PgWorkspaceStore::new(pool.clone()));
        let sessions: Arc<dyn SessionStore> = Arc::new(PgSessionStore::new(pool.clone()));
        let docs: Arc<dyn DocStore> = Arc::new(PgDocStore::new(pool.clone()));
        let grants: Arc<dyn GrantStore> = Arc::new(PgGrantStore::new(pool.clone()));
        let acl = Arc::new(AclCache::new(workspaces.clone(), grants.clone()));
        Self {
            pool: Some(pool),
            users: Some(users),
            workspaces: Some(workspaces),
            sessions: Some(sessions),
            docs: Some(docs),
            grants: Some(grants),
            acl: Some(acl),
            rooms_v2: None,
            bus: None,
            hasher: Arc::new(Hasher::new()),
            throttle: Arc::new(Throttle::new()),
            session_key: Vec::new(),
            base_url: "http://localhost:3000".into(),
            oidc_enabled: false,
            oidc: None,
            config: Arc::new(Config::default()),
        }
    }

    pub fn session_deps(&self) -> Option<SessionDeps> {
        Some(SessionDeps {
            sessions: self.sessions.clone()?,
            workspaces: self.workspaces.clone()?,
        })
    }
}

/// In-memory router (used by tests + the spike main without DB).
pub fn router() -> Router {
    router_with_state(AppState::in_memory())
}

pub fn router_with_state(state: AppState) -> Router {
    let mut r = Router::new()
        .route("/collab/:doc_id", get(collab_upgrade))
        .merge(routes::health::router())
        .merge(routes::auth::router())
        .merge(routes::api::router(state.clone()));

    if let Some(deps) = state.session_deps() {
        r = r.layer(axum::middleware::from_fn_with_state(
            deps,
            auth::session_loader_mw,
        ));
    }

    r.with_state(state)
}

async fn collab_upgrade(
    Path(doc_id): Path<Uuid>,
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
    req: axum::extract::Request,
) -> axum::response::Response {
    let Some(ctx) = req.extensions().get::<crate::auth::AuthContext>().cloned() else {
        return (
            axum::http::StatusCode::UNAUTHORIZED,
            "auth.session_required",
        )
            .into_response();
    };
    let acl = match state.acl.as_ref() {
        Some(a) => a.clone(),
        None => {
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "internal").into_response();
        }
    };
    match acl
        .effective_role(ctx.workspace_id, doc_id, ctx.user_id)
        .await
    {
        Ok(Some(_role)) => {}
        Ok(None) => return (axum::http::StatusCode::FORBIDDEN, "acl.no_grant").into_response(),
        Err(_) => {
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "internal").into_response();
        }
    }
    let rooms = match state.rooms_v2.as_ref() {
        Some(r) => r.clone(),
        None => {
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "internal").into_response();
        }
    };
    ws.on_upgrade(move |socket| async move {
        crate::room::serve(rooms, doc_id, socket).await;
    })
    .into_response()
}
