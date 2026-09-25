use anyhow::Result;
use sqlx::Row;

use super::RuntimeStore;

impl RuntimeStore {
    /// Only the host holding exclusive runtime ownership may retire feature data.
    /// Ordinary store opens can share a profile with an older, still-running host.
    pub async fn retire_removed_features(&self) -> Result<()> {
        let mut transaction = self.pool().begin().await?;
        for statement in [
            // The trigger belongs to workspaceTabs and survives dropping its target table.
            "DROP TRIGGER IF EXISTS codexChatStateDeleteTab",
            "DROP TABLE IF EXISTS agentCanvasEvents",
            "DROP TABLE IF EXISTS agentCanvasDecisions",
            "DROP TABLE IF EXISTS agentCanvasRevisions",
            "DROP TABLE IF EXISTS agentCanvases",
            "DROP TABLE IF EXISTS browserTrustedCertificates",
            "DROP TABLE IF EXISTS browserPermissions",
            "DROP TABLE IF EXISTS browserClosedTabs",
            "DROP TABLE IF EXISTS browserHistory",
            "DROP TABLE IF EXISTS browserProfiles",
            "DROP TABLE IF EXISTS codexChatState",
            "DELETE FROM workspaceTabs WHERE kind IN ('mobileEmulator', 'browser', 'codex')",
        ] {
            sqlx::query(statement).execute(&mut *transaction).await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    /// Rewrites project and workspace roots stored in the Windows verbatim form
    /// (`\\?\E:\...`, from `std::fs::canonicalize`) to the plain form. `cmd.exe`
    /// refuses a verbatim current directory and silently runs in `C:\Windows`,
    /// and every client comparing these roots against paths from git, file
    /// URIs, or pickers would otherwise have to special-case the prefix.
    /// `dunce::simplified` keeps a path verbatim when the plain form would be
    /// ambiguous (too long, reserved names) and is the identity elsewhere, so
    /// this is a no-op outside Windows. Like feature retirement, only the host
    /// holding exclusive runtime ownership may run it.
    pub async fn normalize_verbatim_root_paths(&self) -> Result<()> {
        let mut transaction = self.pool().begin().await?;
        for (table, column) in [("projects", "repoPath"), ("workspaces", "path")] {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "SELECT id, {column} FROM {table}"
            )))
            .fetch_all(&mut *transaction)
            .await?;
            for row in rows {
                let id: String = row.try_get("id")?;
                let stored: String = row.try_get(column)?;
                let plain = dunce::simplified(std::path::Path::new(&stored))
                    .to_string_lossy()
                    .into_owned();
                if plain == stored {
                    continue;
                }
                sqlx::query(sqlx::AssertSqlSafe(format!(
                    "UPDATE {table} SET {column} = ? WHERE id = ?"
                )))
                .bind(plain)
                .bind(id)
                .execute(&mut *transaction)
                .await?;
            }
        }
        transaction.commit().await?;
        Ok(())
    }

    pub(super) async fn ensure_column(
        &self,
        table: &str,
        column: &str,
        definition: &str,
    ) -> Result<()> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!("PRAGMA table_info({table})")))
            .fetch_all(self.pool())
            .await?;
        let exists = rows.iter().any(|row| {
            row.try_get::<String, _>("name")
                .is_ok_and(|name| name == column)
        });
        if !exists {
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "ALTER TABLE {table} ADD COLUMN {column} {definition}"
            )))
            .execute(self.pool())
            .await?;
        }
        Ok(())
    }
}
