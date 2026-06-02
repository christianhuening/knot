//! `knot-server admin create` — headless first-user bootstrap.

use std::io::{Read, Write};
use std::sync::Arc;

use clap::{Args, Subcommand};
use knot_auth::Hasher;
use knot_config::Config;
use knot_storage::{PgUserStore, PgWorkspaceStore, UserStore, WorkspaceRole, WorkspaceStore};

#[derive(Args)]
pub struct AdminArgs {
    #[command(subcommand)]
    pub cmd: AdminCmd,
}

#[derive(Subcommand)]
pub enum AdminCmd {
    /// Create the first user (and the singleton workspace if missing).
    /// Reads the password from stdin so it stays out of shell history.
    Create {
        #[arg(long)]
        email: String,
        #[arg(long)]
        display_name: String,
        /// Workspace name to use when bootstrapping. Ignored if a workspace
        /// already exists.
        #[arg(long, default_value = "Workspace")]
        workspace_name: String,
        /// Workspace slug used at creation.
        #[arg(long, default_value = "default")]
        workspace_slug: String,
    },
}

pub async fn run(cfg: Config, args: AdminArgs) -> anyhow::Result<()> {
    match args.cmd {
        AdminCmd::Create {
            email,
            display_name,
            workspace_name,
            workspace_slug,
        } => create(cfg, &email, &display_name, &workspace_name, &workspace_slug).await,
    }
}

async fn create(
    cfg: Config,
    email: &str,
    display_name: &str,
    workspace_name: &str,
    workspace_slug: &str,
) -> anyhow::Result<()> {
    if cfg.database_url.is_empty() {
        anyhow::bail!("KNOT_DATABASE_URL must be set");
    }
    let pool = knot_storage::connect(&cfg.database_url, 4).await?;
    let users = Arc::new(PgUserStore::new(pool.clone()));
    let ws = Arc::new(PgWorkspaceStore::new(pool));

    let mut buf = String::new();
    write!(std::io::stderr(), "password (read from stdin): ").ok();
    std::io::stderr().flush().ok();
    std::io::stdin().read_to_string(&mut buf)?;
    let password = buf.trim_end_matches(['\n', '\r']);
    if password.len() < 8 {
        anyhow::bail!("password must be at least 8 characters");
    }

    let count = users.count().await?;
    if count > 0 {
        anyhow::bail!("users already exist; admin create is first-run only");
    }

    let hasher = Hasher::new();
    let hash = hasher
        .hash(password)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    let workspace = match ws.get_singleton().await? {
        Some(w) => w,
        None => ws.create(workspace_slug, workspace_name).await?,
    };
    let user = users.create_local(email, display_name, &hash).await?;
    ws.add_member(workspace.id, user.id, WorkspaceRole::Owner)
        .await?;

    println!(
        "created user {} ({}) as owner of workspace {} ({})",
        user.id, email, workspace.id, workspace.slug,
    );
    Ok(())
}
