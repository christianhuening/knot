//! Integration tests for SessionStore against an ephemeral Postgres.

use chrono::{Duration, Utc};
use knot_storage::{
    PgSessionStore, PgUserStore, PgWorkspaceStore, SessionStore, UserStore, WorkspaceRole,
    WorkspaceStore,
};
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};

async fn setup() -> (PgSessionStore, uuid::Uuid, uuid::Uuid) {
    let container = Postgres::default().start().await.expect("pg start");
    let port = container.get_host_port_ipv4(5432).await.expect("port");
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();

    let ws = PgWorkspaceStore::new(pool.clone())
        .create("acme", "Acme")
        .await
        .unwrap();
    let u = PgUserStore::new(pool.clone())
        .create_local("a@x.test", "A", "$h$")
        .await
        .unwrap();
    PgWorkspaceStore::new(pool.clone())
        .add_member(ws.id, u.id, WorkspaceRole::Owner)
        .await
        .unwrap();
    std::mem::forget(container);
    (PgSessionStore::new(pool), u.id, ws.id)
}

#[tokio::test(flavor = "multi_thread")]
async fn create_find_delete() {
    let (s, user_id, ws_id) = setup().await;
    let id = [1u8; 32];
    let exp = Utc::now() + Duration::days(30);

    s.create(&id, user_id, ws_id, exp, Some("ua"), None)
        .await
        .unwrap();
    let found = s.find_active(&id).await.unwrap().expect("some");
    assert_eq!(found.user_id, user_id);

    s.delete(&id).await.unwrap();
    assert!(s.find_active(&id).await.unwrap().is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn expired_sessions_invisible() {
    let (s, user_id, ws_id) = setup().await;
    let id = [2u8; 32];
    let exp = Utc::now() - Duration::seconds(1);
    s.create(&id, user_id, ws_id, exp, None, None)
        .await
        .unwrap();
    assert!(s.find_active(&id).await.unwrap().is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn touch_updates_last_seen() {
    let (s, user_id, ws_id) = setup().await;
    let id = [3u8; 32];
    let exp = Utc::now() + Duration::days(30);
    let created = s
        .create(&id, user_id, ws_id, exp, None, None)
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    s.touch(&id).await.unwrap();
    let after = s.find_active(&id).await.unwrap().unwrap();
    assert!(after.last_seen_at > created.last_seen_at);
}
