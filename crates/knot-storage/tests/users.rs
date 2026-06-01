use knot_storage::{PgUserStore, UserStore, UserStoreError};
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};

async fn fresh_store() -> PgUserStore {
    let container = Postgres::default().start().await.expect("pg start");
    let port = container.get_host_port_ipv4(5432).await.expect("port");
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .expect("pool");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("migrate");
    // Leak the container handle. testcontainers' ryuk reaper kills it after
    // the test process exits, which is what we want for multi-test files.
    std::mem::forget(container);
    PgUserStore::new(pool)
}

#[tokio::test(flavor = "multi_thread")]
async fn local_user_lifecycle() {
    let s = fresh_store().await;
    assert_eq!(s.count().await.unwrap(), 0);

    let u = s
        .create_local("alice@example.com", "Alice", "$argon2id$dummy")
        .await
        .expect("create");
    assert_eq!(u.email, "alice@example.com");
    assert_eq!(u.display_name, "Alice");
    assert!(u.password_hash.is_some());

    // citext: lookup is case-insensitive.
    let found = s.find_by_email("ALICE@example.com").await.unwrap();
    assert_eq!(found.map(|f| f.id), Some(u.id));

    // find_by_id works.
    let by_id = s.find_by_id(u.id).await.unwrap();
    assert_eq!(
        by_id.map(|f| f.email),
        Some("alice@example.com".to_string())
    );

    assert_eq!(s.count().await.unwrap(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn duplicate_email_rejected() {
    let s = fresh_store().await;
    s.create_local("a@x.test", "A", "$h$").await.unwrap();
    let err = s
        .create_local("a@x.test", "A2", "$h$")
        .await
        .expect_err("must fail");
    assert!(matches!(err, UserStoreError::EmailExists), "got {err:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn oidc_user_lookup() {
    let s = fresh_store().await;
    let u = s
        .create_oidc("alice@example.com", "Alice", "http://dex/dex", "08a86")
        .await
        .unwrap();
    let found = s.find_by_oidc("http://dex/dex", "08a86").await.unwrap();
    assert_eq!(found.map(|f| f.id), Some(u.id));
}

#[tokio::test(flavor = "multi_thread")]
async fn duplicate_oidc_rejected() {
    let s = fresh_store().await;
    s.create_oidc("a@x.test", "A", "iss", "sub").await.unwrap();
    let err = s
        .create_oidc("b@x.test", "B", "iss", "sub")
        .await
        .expect_err("must fail");
    assert!(matches!(err, UserStoreError::OidcExists), "got {err:?}");
}
