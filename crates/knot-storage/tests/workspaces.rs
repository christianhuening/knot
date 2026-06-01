//! Integration test for WorkspaceStore against an ephemeral Postgres.

use knot_storage::{PgWorkspaceStore, WorkspaceRole, WorkspaceStore};
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};
use uuid::Uuid;

#[tokio::test(flavor = "multi_thread")]
async fn workspace_crud_roundtrip() {
    let container = Postgres::default().start().await.expect("pg start");
    let host_port = container.get_host_port_ipv4(5432).await.expect("host port");
    let url = format!("postgres://postgres:postgres@127.0.0.1:{host_port}/postgres");

    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .expect("pool");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("migrate");

    let store = PgWorkspaceStore::new(pool.clone());

    // Empty → no singleton.
    assert!(store.get_singleton().await.unwrap().is_none());

    // Create.
    let ws = store.create("acme", "Acme Co").await.expect("create");
    assert_eq!(ws.slug, "acme");
    assert_eq!(ws.name, "Acme Co");

    // get_singleton returns it.
    let s = store.get_singleton().await.unwrap().expect("some");
    assert_eq!(s.id, ws.id);

    // Add a member.
    let user_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO users (email, display_name) VALUES ('a@x.test', 'A')
         RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("create user");

    store
        .add_member(ws.id, user_id, WorkspaceRole::Owner)
        .await
        .expect("add member");

    let role = store.get_member_role(ws.id, user_id).await.unwrap();
    assert_eq!(role, Some(WorkspaceRole::Owner));

    // Missing member: None.
    let other = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO users (email, display_name) VALUES ('b@x.test', 'B')
         RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(store.get_member_role(ws.id, other).await.unwrap(), None);
}
