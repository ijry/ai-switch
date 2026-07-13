use crate::error::AppError;
use crate::models::mcp::{McpServer, NewMcpServer};
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct McpRepository;

impl McpRepository {
    pub async fn create(pool: &SqlitePool, input: NewMcpServer) -> Result<McpServer, AppError> {
        let now = Utc::now().to_rfc3339();
        let id = Uuid::new_v4().to_string();
        let enabled = i64::from(input.enabled);

        sqlx::query(
            "INSERT INTO mcp_servers (id, name, transport, command, args_json, url, env_json, enabled, notes, status, sort_order, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'configured', 0, ?, ?)",
        )
        .bind(&id)
        .bind(&input.name)
        .bind(&input.transport)
        .bind(&input.command)
        .bind(&input.args_json)
        .bind(&input.url)
        .bind(&input.env_json)
        .bind(enabled)
        .bind(&input.notes)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.mcp_create",
            message: "Could not create MCP server".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Self::get(pool, &id).await
    }

    pub async fn get(pool: &SqlitePool, id: &str) -> Result<McpServer, AppError> {
        sqlx::query_as::<_, McpServer>("SELECT * FROM mcp_servers WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.mcp_get",
                message: "Could not load MCP server".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }

    pub async fn list(pool: &SqlitePool) -> Result<Vec<McpServer>, AppError> {
        sqlx::query_as::<_, McpServer>(
            "SELECT * FROM mcp_servers ORDER BY sort_order ASC, created_at DESC",
        )
        .fetch_all(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.mcp_list",
            message: "Could not list MCP servers".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })
    }

    pub async fn set_enabled(
        pool: &SqlitePool,
        id: &str,
        enabled: bool,
    ) -> Result<McpServer, AppError> {
        let now = Utc::now().to_rfc3339();
        let enabled = i64::from(enabled);

        sqlx::query("UPDATE mcp_servers SET enabled = ?, updated_at = ? WHERE id = ?")
            .bind(enabled)
            .bind(&now)
            .bind(id)
            .execute(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.mcp_update_enabled",
                message: "Could not update MCP server".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;

        Self::get(pool, id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{create_memory_pool, run_migrations};

    #[tokio::test]
    async fn list_returns_mcp_servers_ordered_by_created_at() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        McpRepository::create(
            &pool,
            NewMcpServer {
                name: "Filesystem".to_string(),
                transport: "stdio".to_string(),
                command: Some("npx".to_string()),
                args_json: "[\"-y\",\"@modelcontextprotocol/server-filesystem\"]".to_string(),
                url: None,
                env_json: "{}".to_string(),
                enabled: true,
                notes: Some("Local files".to_string()),
            },
        )
        .await
        .expect("first");
        McpRepository::create(
            &pool,
            NewMcpServer {
                name: "Docs".to_string(),
                transport: "sse".to_string(),
                command: None,
                args_json: "[]".to_string(),
                url: Some("https://mcp.example.com/sse".to_string()),
                env_json: "{}".to_string(),
                enabled: false,
                notes: None,
            },
        )
        .await
        .expect("second");

        let servers = McpRepository::list(&pool).await.expect("servers");

        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0].name, "Docs");
        assert_eq!(servers[1].name, "Filesystem");
    }

    #[tokio::test]
    async fn set_enabled_updates_server_state() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let server = McpRepository::create(
            &pool,
            NewMcpServer {
                name: "Filesystem".to_string(),
                transport: "stdio".to_string(),
                command: Some("npx".to_string()),
                args_json: "[]".to_string(),
                url: None,
                env_json: "{}".to_string(),
                enabled: true,
                notes: None,
            },
        )
        .await
        .expect("server");

        let disabled = McpRepository::set_enabled(&pool, &server.id, false)
            .await
            .expect("disabled");

        assert_eq!(disabled.enabled, 0);
    }
}
