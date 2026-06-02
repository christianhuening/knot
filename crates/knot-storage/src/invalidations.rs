//! ACL invalidations outbox. Rows written in the same transaction as the
//! mutation; consumed by the listener in knot-docs.

use sqlx::PgConnection;
use uuid::Uuid;

pub async fn record_in_tx(
    tx: &mut PgConnection,
    workspace_id: Uuid,
    doc_id: Uuid,
    reason: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO acl_invalidations (workspace_id, doc_id, reason)
         VALUES ($1, $2, $3)",
    )
    .bind(workspace_id)
    .bind(doc_id)
    .bind(reason)
    .execute(&mut *tx)
    .await?;
    // Notify listeners. Payload = doc_id text so listener can target evictions.
    // NOTE: NOTIFY can't be parameterised; doc_id comes from internal call
    // sites only (never user input), so format!() is safe here.
    sqlx::query(&format!("NOTIFY acl_invalidate, '{}'", doc_id))
        .execute(&mut *tx)
        .await?;
    Ok(())
}
