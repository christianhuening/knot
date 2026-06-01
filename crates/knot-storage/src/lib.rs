//! Storage layer for knot — Postgres pool + storage traits.

pub mod doc_store;
pub mod pool;
pub mod workspace_store;

pub use doc_store::{DocStore, DocStoreError};
pub use pool::{Pool, PoolError, connect};
pub use workspace_store::{
    PgWorkspaceStore, Workspace, WorkspaceRole, WorkspaceStore, WorkspaceStoreError,
};
