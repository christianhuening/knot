use knot_storage::{
    DocStore, PgDocStore, PgUserStore, PgWorkspaceStore, UserStore, WorkspaceRole, WorkspaceStore,
    sort_key_between,
};
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};
use uuid::Uuid;

async fn setup() -> (PgDocStore, Uuid, Uuid) {
    let container = Postgres::default().start().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    std::mem::forget(container);

    let ws = PgWorkspaceStore::new(pool.clone())
        .create("default", "W")
        .await
        .unwrap();
    let users = PgUserStore::new(pool.clone());
    let u = users.create_local("a@x.test", "A", "$h$").await.unwrap();
    PgWorkspaceStore::new(pool.clone())
        .add_member(ws.id, u.id, WorkspaceRole::Owner)
        .await
        .unwrap();
    (PgDocStore::new(pool), ws.id, u.id)
}

#[tokio::test(flavor = "multi_thread")]
async fn create_get_list_lifecycle() {
    let (store, ws, user) = setup().await;
    let sk = sort_key_between(None, None);
    let doc = store.create(ws, None, "Hello", &sk, user).await.unwrap();
    assert_eq!(doc.title, "Hello");
    assert_eq!(doc.workspace_id, ws);
    let got = store.get(doc.id).await.unwrap().unwrap();
    assert_eq!(got.id, doc.id);
    let list = store.list_alive(ws).await.unwrap();
    assert_eq!(list.len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn rename_updates_title_and_icon() {
    let (store, ws, user) = setup().await;
    let sk = sort_key_between(None, None);
    let doc = store.create(ws, None, "Old", &sk, user).await.unwrap();
    let new = store
        .rename(ws, doc.id, user, "New", Some("📄"))
        .await
        .unwrap();
    assert_eq!(new.title, "New");
    assert_eq!(new.icon.as_deref(), Some("📄"));
}

#[tokio::test(flavor = "multi_thread")]
async fn archive_hides_and_restore_brings_back() {
    let (store, ws, user) = setup().await;
    let sk = sort_key_between(None, None);
    let doc = store.create(ws, None, "X", &sk, user).await.unwrap();
    store.archive(ws, doc.id, user).await.unwrap();
    assert_eq!(store.list_alive(ws).await.unwrap().len(), 0);
    store.restore(ws, doc.id, user).await.unwrap();
    assert_eq!(store.list_alive(ws).await.unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn move_to_under_new_parent() {
    let (store, ws, user) = setup().await;
    let a = store.create(ws, None, "A", "m", user).await.unwrap();
    let b = store.create(ws, None, "B", "n", user).await.unwrap();
    let moved = store
        .move_to(ws, b.id, user, Some(a.id), "m")
        .await
        .unwrap();
    assert_eq!(moved.parent_id, Some(a.id));
    let kids = store.siblings(ws, Some(a.id)).await.unwrap();
    assert_eq!(kids.len(), 1);
    assert_eq!(kids[0].id, b.id);
}

#[tokio::test(flavor = "multi_thread")]
async fn rename_not_found() {
    let (store, ws, user) = setup().await;
    let err = store
        .rename(ws, Uuid::new_v4(), user, "X", None)
        .await
        .unwrap_err();
    assert!(matches!(err, knot_storage::DocStoreError::NotFound));
}
