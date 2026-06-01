//! Users CRUD — local credentials and OIDC linkage.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub password_hash: Option<String>,
    pub oidc_subject: Option<String>,
    pub oidc_issuer: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Error)]
pub enum UserStoreError {
    #[error("sqlx: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("email already exists")]
    EmailExists,
    #[error("oidc subject already linked")]
    OidcExists,
}

#[async_trait]
pub trait UserStore: Send + Sync + 'static {
    async fn create_local(
        &self,
        email: &str,
        display_name: &str,
        password_hash: &str,
    ) -> Result<User, UserStoreError>;

    async fn create_oidc(
        &self,
        email: &str,
        display_name: &str,
        issuer: &str,
        subject: &str,
    ) -> Result<User, UserStoreError>;

    async fn find_by_email(&self, email: &str) -> Result<Option<User>, UserStoreError>;
    async fn find_by_oidc(
        &self,
        issuer: &str,
        subject: &str,
    ) -> Result<Option<User>, UserStoreError>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, UserStoreError>;
    async fn count(&self) -> Result<i64, UserStoreError>;
}

#[derive(Clone)]
pub struct PgUserStore {
    pool: PgPool,
}

impl PgUserStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

type UserRow = (
    Uuid,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    DateTime<Utc>,
);

fn user_from_row(r: UserRow) -> User {
    User {
        id: r.0,
        email: r.1,
        display_name: r.2,
        password_hash: r.3,
        oidc_subject: r.4,
        oidc_issuer: r.5,
        created_at: r.6,
    }
}

fn map_unique_violation_email(e: sqlx::Error) -> UserStoreError {
    match e {
        sqlx::Error::Database(ref db) if db.is_unique_violation() => UserStoreError::EmailExists,
        e => UserStoreError::Sqlx(e),
    }
}

fn map_unique_violation_oidc(e: sqlx::Error) -> UserStoreError {
    match e {
        sqlx::Error::Database(ref db) if db.is_unique_violation() => UserStoreError::OidcExists,
        e => UserStoreError::Sqlx(e),
    }
}

const SELECT_USER_COLS: &str =
    "id, email::text, display_name, password_hash, oidc_subject, oidc_issuer, created_at";

#[async_trait]
impl UserStore for PgUserStore {
    async fn create_local(
        &self,
        email: &str,
        display_name: &str,
        password_hash: &str,
    ) -> Result<User, UserStoreError> {
        let row = sqlx::query_as::<_, UserRow>(&format!(
            "INSERT INTO users (email, display_name, password_hash)
             VALUES ($1, $2, $3)
             RETURNING {SELECT_USER_COLS}"
        ))
        .bind(email)
        .bind(display_name)
        .bind(password_hash)
        .fetch_one(&self.pool)
        .await
        .map_err(map_unique_violation_email)?;
        Ok(user_from_row(row))
    }

    async fn create_oidc(
        &self,
        email: &str,
        display_name: &str,
        issuer: &str,
        subject: &str,
    ) -> Result<User, UserStoreError> {
        let row = sqlx::query_as::<_, UserRow>(&format!(
            "INSERT INTO users (email, display_name, oidc_issuer, oidc_subject)
             VALUES ($1, $2, $3, $4)
             RETURNING {SELECT_USER_COLS}"
        ))
        .bind(email)
        .bind(display_name)
        .bind(issuer)
        .bind(subject)
        .fetch_one(&self.pool)
        .await
        .map_err(map_unique_violation_oidc)?;
        Ok(user_from_row(row))
    }

    async fn find_by_email(&self, email: &str) -> Result<Option<User>, UserStoreError> {
        // Cast the bound parameter to citext so the comparison uses
        // citext (case-insensitive) semantics; binding a Rust &str produces
        // a plain `text` parameter which would do a case-sensitive compare.
        let row = sqlx::query_as::<_, UserRow>(&format!(
            "SELECT {SELECT_USER_COLS} FROM users WHERE email = $1::citext"
        ))
        .bind(email)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(user_from_row))
    }

    async fn find_by_oidc(
        &self,
        issuer: &str,
        subject: &str,
    ) -> Result<Option<User>, UserStoreError> {
        let row = sqlx::query_as::<_, UserRow>(&format!(
            "SELECT {SELECT_USER_COLS} FROM users
             WHERE oidc_issuer = $1 AND oidc_subject = $2"
        ))
        .bind(issuer)
        .bind(subject)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(user_from_row))
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, UserStoreError> {
        let row = sqlx::query_as::<_, UserRow>(&format!(
            "SELECT {SELECT_USER_COLS} FROM users WHERE id = $1"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(user_from_row))
    }

    async fn count(&self) -> Result<i64, UserStoreError> {
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&self.pool)
            .await?;
        Ok(n)
    }
}
