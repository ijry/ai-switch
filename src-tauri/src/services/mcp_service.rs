use crate::database::repositories::mcp_repository::McpRepository;
use crate::error::AppError;
use crate::models::mcp::{McpServer, NewMcpServer, SetMcpServerEnabledRequest};
use serde_json::Value;
use sqlx::SqlitePool;

pub struct McpService;

impl McpService {
    pub async fn list_mcp_servers(pool: &SqlitePool) -> Result<Vec<McpServer>, AppError> {
        McpRepository::list(pool).await
    }

    pub async fn create_mcp_server(
        pool: &SqlitePool,
        input: NewMcpServer,
    ) -> Result<McpServer, AppError> {
        let normalized = normalize_mcp_server(input)?;
        McpRepository::create(pool, normalized).await
    }

    pub async fn set_mcp_server_enabled(
        pool: &SqlitePool,
        request: SetMcpServerEnabledRequest,
    ) -> Result<McpServer, AppError> {
        let id = request.id.trim();
        if id.is_empty() {
            return Err(AppError::Validation {
                code: "validation.mcp_id_required",
                message: "MCP server id is required".to_string(),
                details: None,
                recoverable: true,
            });
        }

        McpRepository::set_enabled(pool, id, request.enabled).await
    }
}

fn normalize_mcp_server(input: NewMcpServer) -> Result<NewMcpServer, AppError> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::Validation {
            code: "validation.mcp_name_required",
            message: "MCP server name is required".to_string(),
            details: None,
            recoverable: true,
        });
    }

    let transport = normalize_transport(&input.transport)?;
    let args_json = normalize_args_json(&input.args_json)?;
    let env_json = normalize_env_json(&input.env_json)?;
    let notes = input
        .notes
        .and_then(|notes| non_empty_string(notes.trim().to_string()));

    let command = input
        .command
        .and_then(|command| non_empty_string(command.trim().to_string()));
    let url = input
        .url
        .and_then(|url| non_empty_string(url.trim().to_string()));

    let (command, url) = match transport.as_str() {
        "stdio" => {
            let Some(command) = command else {
                return Err(AppError::Validation {
                    code: "validation.mcp_command_required",
                    message: "Stdio MCP servers require a command".to_string(),
                    details: None,
                    recoverable: true,
                });
            };
            (Some(command), None)
        }
        "sse" | "streamable_http" => {
            let Some(url) = url else {
                return Err(AppError::Validation {
                    code: "validation.mcp_url_required",
                    message: "URL-based MCP servers require a URL".to_string(),
                    details: None,
                    recoverable: true,
                });
            };
            if !url.starts_with("https://") && !url.starts_with("http://") {
                return Err(AppError::Validation {
                    code: "validation.mcp_url_scheme",
                    message: "MCP server URL must start with http:// or https://".to_string(),
                    details: Some(url),
                    recoverable: true,
                });
            }
            (None, Some(url))
        }
        _ => unreachable!("transport normalized before match"),
    };

    Ok(NewMcpServer {
        name,
        transport,
        command,
        args_json,
        url,
        env_json,
        enabled: input.enabled,
        notes,
    })
}

fn normalize_transport(transport: &str) -> Result<String, AppError> {
    let normalized = transport.trim().to_lowercase();
    if matches!(normalized.as_str(), "stdio" | "sse" | "streamable_http") {
        return Ok(normalized);
    }

    Err(AppError::Validation {
        code: "validation.mcp_transport",
        message: "MCP transport must be stdio, sse, or streamable_http".to_string(),
        details: Some(transport.to_string()),
        recoverable: true,
    })
}

fn normalize_args_json(args_json: &str) -> Result<String, AppError> {
    let value = parse_json_or_default(args_json, "[]", "validation.mcp_args_json")?;
    if !value.is_array() {
        return Err(AppError::Validation {
            code: "validation.mcp_args_array",
            message: "MCP args JSON must be an array".to_string(),
            details: None,
            recoverable: true,
        });
    }
    serde_json::to_string(&value).map_err(AppError::from)
}

fn normalize_env_json(env_json: &str) -> Result<String, AppError> {
    let value = parse_json_or_default(env_json, "{}", "validation.mcp_env_json")?;
    let Some(env) = value.as_object() else {
        return Err(AppError::Validation {
            code: "validation.mcp_env_object",
            message: "MCP environment JSON must be an object".to_string(),
            details: None,
            recoverable: true,
        });
    };

    for (key, value) in env {
        let Some(value) = value.as_str() else {
            return Err(AppError::Validation {
                code: "validation.mcp_env_string_values",
                message: "MCP environment values must be strings".to_string(),
                details: Some(key.clone()),
                recoverable: true,
            });
        };

        if is_sensitive_key(key) && !is_secret_reference(value) {
            return Err(AppError::Validation {
                code: "validation.mcp_env_secret_ref_required",
                message: "Sensitive MCP environment values must use env:// or secret:// references"
                    .to_string(),
                details: Some(key.clone()),
                recoverable: true,
            });
        }
    }

    serde_json::to_string(&value).map_err(AppError::from)
}

fn parse_json_or_default(
    json: &str,
    default_json: &str,
    code: &'static str,
) -> Result<Value, AppError> {
    let json = if json.trim().is_empty() {
        default_json
    } else {
        json.trim()
    };

    serde_json::from_str(json).map_err(|error| AppError::Validation {
        code,
        message: "MCP JSON field is invalid".to_string(),
        details: Some(error.to_string()),
        recoverable: true,
    })
}

fn is_sensitive_key(key: &str) -> bool {
    let normalized = key.to_lowercase();
    ["token", "api_key", "apikey", "password", "secret"]
        .iter()
        .any(|needle| normalized.contains(needle))
}

fn is_secret_reference(value: &str) -> bool {
    value.starts_with("env://") || value.starts_with("secret://")
}

fn non_empty_string(value: String) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{create_memory_pool, run_migrations};

    #[tokio::test]
    async fn create_mcp_server_normalizes_stdio_config() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let server = McpService::create_mcp_server(
            &pool,
            NewMcpServer {
                name: " Filesystem ".to_string(),
                transport: "STDIO".to_string(),
                command: Some(" npx ".to_string()),
                args_json: "[\"-y\",\"@modelcontextprotocol/server-filesystem\"]".to_string(),
                url: Some("https://ignored.example.com".to_string()),
                env_json: "{\"LOG_LEVEL\":\"debug\",\"BRAVE_API_KEY\":\"env://BRAVE_API_KEY\"}"
                    .to_string(),
                enabled: true,
                notes: Some(" Local files ".to_string()),
            },
        )
        .await
        .expect("server");

        assert_eq!(server.name, "Filesystem");
        assert_eq!(server.transport, "stdio");
        assert_eq!(server.command.as_deref(), Some("npx"));
        assert_eq!(server.url, None);
        assert_eq!(server.enabled, 1);
        assert_eq!(server.notes.as_deref(), Some("Local files"));
    }

    #[tokio::test]
    async fn create_mcp_server_rejects_raw_secret_env_values() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let error = McpService::create_mcp_server(
            &pool,
            NewMcpServer {
                name: "Unsafe".to_string(),
                transport: "stdio".to_string(),
                command: Some("node".to_string()),
                args_json: "[]".to_string(),
                url: None,
                env_json: "{\"API_KEY\":\"raw-token\"}".to_string(),
                enabled: true,
                notes: None,
            },
        )
        .await
        .expect_err("error");

        assert_eq!(error.code(), "validation.mcp_env_secret_ref_required");
    }

    #[tokio::test]
    async fn create_mcp_server_rejects_missing_url_for_sse() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let error = McpService::create_mcp_server(
            &pool,
            NewMcpServer {
                name: "Remote".to_string(),
                transport: "sse".to_string(),
                command: None,
                args_json: "[]".to_string(),
                url: None,
                env_json: "{}".to_string(),
                enabled: true,
                notes: None,
            },
        )
        .await
        .expect_err("error");

        assert_eq!(error.code(), "validation.mcp_url_required");
    }
}
