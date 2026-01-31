use axum::{
    extract::State,
    http::StatusCode,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::{collections::HashMap, env, sync::Arc};
use tokio::sync::RwLock;

mod apns;

use apns::ApnsClients;

#[derive(Clone)]
struct AppState {
    db: SqlitePool,
    apns: Arc<RwLock<ApnsClients>>,
}

#[derive(Debug, Deserialize)]
struct RegisterRequest {
    device_token: String,
    environment: Environment,
    device_name: Option<String>,
    device_type: Option<String>,
    os_version: Option<String>,
    app_version: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Environment {
    Sandbox,
    Production,
}

impl Environment {
    fn as_str(&self) -> &'static str {
        match self {
            Environment::Sandbox => "sandbox",
            Environment::Production => "production",
        }
    }
}

impl TryFrom<&str> for Environment {
    type Error = &'static str;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "sandbox" => Ok(Environment::Sandbox),
            "production" => Ok(Environment::Production),
            _ => Err("invalid environment"),
        }
    }
}

#[derive(Debug, Serialize)]
struct RegisterResponse {
    success: bool,
    message: String,
}

#[derive(Debug, Deserialize)]
struct SendRequest {
    device_token: String,

    // Alert options
    title: Option<String>,
    subtitle: Option<String>,
    body: Option<String>,
    launch_image: Option<String>,

    // Localization
    title_loc_key: Option<String>,
    title_loc_args: Option<Vec<String>>,
    loc_key: Option<String>,
    loc_args: Option<Vec<String>>,

    // Badge & Sound
    badge: Option<u32>,
    sound: Option<SoundConfig>,

    // Behavior
    content_available: Option<bool>,
    mutable_content: Option<bool>,
    category: Option<String>,

    // Delivery options
    priority: Option<u8>,
    collapse_id: Option<String>,
    expiration: Option<u64>,

    // Custom data
    data: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum SoundConfig {
    Simple(String),
    Critical {
        name: String,
        critical: Option<bool>,
        volume: Option<f64>,
    },
}

#[derive(Debug, Serialize)]
struct SendResponse {
    success: bool,
    message: String,
    apns_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    success: bool,
    error: String,
}

async fn register_device(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> Result<Json<RegisterResponse>, (StatusCode, Json<ErrorResponse>)> {
    let result = sqlx::query(
        r#"
        INSERT INTO devices (device_token, environment, device_name, device_type, os_version, app_version, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP)
        ON CONFLICT(device_token) DO UPDATE SET
            environment = excluded.environment,
            device_name = excluded.device_name,
            device_type = excluded.device_type,
            os_version = excluded.os_version,
            app_version = excluded.app_version,
            updated_at = CURRENT_TIMESTAMP
        "#,
    )
    .bind(&req.device_token)
    .bind(req.environment.as_str())
    .bind(&req.device_name)
    .bind(&req.device_type)
    .bind(&req.os_version)
    .bind(&req.app_version)
    .execute(&state.db)
    .await;

    match result {
        Ok(_) => Ok(Json(RegisterResponse {
            success: true,
            message: "Device registered successfully".to_string(),
        })),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                success: false,
                error: format!("Failed to register device: {}", e),
            }),
        )),
    }
}

async fn send_notification(
    State(state): State<AppState>,
    Json(req): Json<SendRequest>,
) -> Result<Json<SendResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Look up device to get environment
    let device: Option<(String,)> = sqlx::query_as(
        "SELECT environment FROM devices WHERE device_token = ?",
    )
    .bind(&req.device_token)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                success: false,
                error: format!("Database error: {}", e),
            }),
        )
    })?;

    let environment = match device {
        Some((env_str,)) => Environment::try_from(env_str.as_str()).map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    success: false,
                    error: "Invalid environment in database".to_string(),
                }),
            )
        })?,
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    success: false,
                    error: "Device not registered".to_string(),
                }),
            ))
        }
    };

    // Build and send notification
    let apns_clients = state.apns.read().await;
    let result = apns_clients
        .send_notification(&req, environment)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    success: false,
                    error: format!("Failed to send notification: {}", e),
                }),
            )
        })?;

    Ok(Json(SendResponse {
        success: true,
        message: "Notification sent successfully".to_string(),
        apns_id: Some(result),
    }))
}

async fn init_db(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS devices (
            device_token TEXT PRIMARY KEY,
            environment TEXT NOT NULL CHECK(environment IN ('sandbox', 'production')),
            device_name TEXT,
            device_type TEXT,
            os_version TEXT,
            app_version TEXT,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:data.db".to_string());

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    init_db(&pool).await?;

    let apns_clients = ApnsClients::new()?;

    let state = AppState {
        db: pool,
        apns: Arc::new(RwLock::new(apns_clients)),
    };

    let app = Router::new()
        .route("/register", post(register_device))
        .route("/send", post(send_notification))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    tracing::info!("Server listening on {}", listener.local_addr()?);

    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_environment_from_str() {
        assert_eq!(Environment::try_from("sandbox").unwrap(), Environment::Sandbox);
        assert_eq!(Environment::try_from("production").unwrap(), Environment::Production);
        assert!(Environment::try_from("invalid").is_err());
    }

    #[test]
    fn test_environment_as_str() {
        assert_eq!(Environment::Sandbox.as_str(), "sandbox");
        assert_eq!(Environment::Production.as_str(), "production");
    }

    #[test]
    fn test_deserialize_register_request() {
        let json = r#"{
            "device_token": "abc123",
            "environment": "sandbox",
            "device_name": "John's iPhone"
        }"#;
        let req: RegisterRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.device_token, "abc123");
        assert_eq!(req.environment, Environment::Sandbox);
        assert_eq!(req.device_name, Some("John's iPhone".to_string()));
    }

    #[test]
    fn test_deserialize_send_request_simple() {
        let json = r#"{
            "device_token": "abc123",
            "title": "Hello",
            "body": "World"
        }"#;
        let req: SendRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.device_token, "abc123");
        assert_eq!(req.title, Some("Hello".to_string()));
        assert_eq!(req.body, Some("World".to_string()));
    }

    #[test]
    fn test_deserialize_send_request_with_sound() {
        let json = r#"{
            "device_token": "abc123",
            "sound": "default"
        }"#;
        let req: SendRequest = serde_json::from_str(json).unwrap();
        assert!(matches!(req.sound, Some(SoundConfig::Simple(s)) if s == "default"));

        let json = r#"{
            "device_token": "abc123",
            "sound": {"name": "alert.caf", "critical": true, "volume": 0.8}
        }"#;
        let req: SendRequest = serde_json::from_str(json).unwrap();
        assert!(matches!(
            req.sound,
            Some(SoundConfig::Critical { name, critical: Some(true), volume: Some(v) })
            if name == "alert.caf" && (v - 0.8).abs() < f64::EPSILON
        ));
    }

    #[test]
    fn test_deserialize_send_request_with_data() {
        let json = r#"{
            "device_token": "abc123",
            "data": {"key": "value", "number": 42}
        }"#;
        let req: SendRequest = serde_json::from_str(json).unwrap();
        let data = req.data.unwrap();
        assert_eq!(data.get("key").unwrap(), "value");
        assert_eq!(data.get("number").unwrap(), 42);
    }
}
