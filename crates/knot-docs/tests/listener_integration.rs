//! Verifies end-to-end: grant change → NOTIFY → listener → cache evict.

use std::sync::Arc;
use std::time::Duration;

use knot_docs::{AclCache, spawn_listener};
use knot_storage::{
    DocStore, GrantStore, PgDocStore, PgGrantStore, PgUserStore, PgWorkspaceStore, UserStore,
    WorkspaceRole, WorkspaceStore,
};
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};

#[tokio::test(flavor = "multi_thread")]
async fn grant_change_evicts_cache_entry() {
    let container = Postgres::default().start().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(&url)
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    std::mem::forget(container);

    let ws_s = PgWorkspaceStore::new(pool.clone());
    let us = PgUserStore::new(pool.clone());
    let ds = PgDocStore::new(pool.clone());
    let gs = PgGrantStore::new(pool.clone());

    let ws = ws_s.create("default", "W").await.unwrap();
    let u = us.create_local("a@x.test", "A", "$h$").await.unwrap();
    ws_s.add_member(ws.id, u.id, WorkspaceRole::Viewer)
        .await
        .unwrap();
    let d = ds.create(ws.id, None, "X", "m", u.id).await.unwrap();

    let cache = Arc::new(AclCache::new(Arc::new(ws_s.clone()), Arc::new(gs.clone())));
    let _handle = spawn_listener(pool.clone(), cache.clone());
    // Let the listener subscribe before emitting NOTIFYs.
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Prime the cache via a read.
    let r1 = cache.effective_role(ws.id, d.id, u.id).await.unwrap();
    assert_eq!(r1, Some(WorkspaceRole::Viewer));

    // Grant upgrade emits NOTIFY (via GrantStore::put → invalidations::record_in_tx).
    gs.put(
        ws.id,
        d.id,
        &format!("user:{}", u.id),
        WorkspaceRole::Owner,
        true,
        u.id,
    )
    .await
    .unwrap();
    // Wait for the listener to receive + process the NOTIFY.
    tokio::time::sleep(Duration::from_millis(800)).await;

    // Re-resolve — the cache entry for (doc, user) should have been evicted,
    // so the read goes back to GrantStore and sees the new role.
    let r2 = cache.effective_role(ws.id, d.id, u.id).await.unwrap();
    assert_eq!(r2, Some(WorkspaceRole::Owner));
}
