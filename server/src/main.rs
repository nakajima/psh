use axum::{
    body::Bytes,
    extract::{Query, State},
    http::{header::CONTENT_TYPE, HeaderMap, StatusCode},
    routing::{get, post},
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
    installation_id: String,
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
    interruption_level: Option<String>,
    relevance_score: Option<f64>,

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
    sent: usize,
    failed: usize,
    results: Vec<DeviceSendResult>,
}

#[derive(Debug, Serialize)]
struct DeviceSendResult {
    device_token: String,
    success: bool,
    apns_id: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    success: bool,
    error: String,
}

#[derive(Debug, Serialize)]
struct StatsResponse {
    total_devices: i64,
    sandbox_devices: i64,
    production_devices: i64,
    total_pushes: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct PushRecord {
    id: i64,
    device_token: String,
    apns_id: Option<String>,
    title: Option<String>,
    body: Option<String>,
    payload: Option<String>,
    sent_at: String,
}

#[derive(Debug, Serialize)]
struct PushesResponse {
    pushes: Vec<PushRecord>,
}

#[derive(Debug, Deserialize)]
struct PushesQuery {
    installation_id: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct PushDetailRecord {
    id: i64,
    apns_id: Option<String>,
    title: Option<String>,
    body: Option<String>,
    payload: Option<String>,
    sent_at: String,
    device_token: String,
    device_name: Option<String>,
    device_type: Option<String>,
    environment: Option<String>,
}

async fn register_device(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> Result<Json<RegisterResponse>, (StatusCode, Json<ErrorResponse>)> {
    tracing::info!(
        device_token = %req.device_token,
        installation_id = %req.installation_id,
        environment = %req.environment.as_str(),
        device_name = ?req.device_name,
        "Registering device"
    );

    let result = sqlx::query(
        r#"
        INSERT INTO devices (device_token, installation_id, environment, device_name, device_type, os_version, app_version, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP)
        ON CONFLICT(device_token) DO UPDATE SET
            installation_id = excluded.installation_id,
            environment = excluded.environment,
            device_name = excluded.device_name,
            device_type = excluded.device_type,
            os_version = excluded.os_version,
            app_version = excluded.app_version,
            updated_at = CURRENT_TIMESTAMP
        "#,
    )
    .bind(&req.device_token)
    .bind(&req.installation_id)
    .bind(req.environment.as_str())
    .bind(&req.device_name)
    .bind(&req.device_type)
    .bind(&req.os_version)
    .bind(&req.app_version)
    .execute(&state.db)
    .await;

    match result {
        Ok(_) => {
            tracing::info!(device_token = %req.device_token, "Device registered");
            Ok(Json(RegisterResponse {
                success: true,
                message: "Device registered successfully".to_string(),
            }))
        }
        Err(e) => {
            tracing::error!(device_token = %req.device_token, error = %e, "Failed to register device");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    success: false,
                    error: format!("Failed to register device: {}", e),
                }),
            ))
        }
    }
}

async fn send_notification(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<SendResponse>, (StatusCode, Json<ErrorResponse>)> {
    let is_json = headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.contains("application/json"))
        .unwrap_or(false);

    tracing::info!(is_json = is_json, body_len = body.len(), "Received send request");

    let req: SendRequest = if is_json {
        serde_json::from_slice(&body).map_err(|e| {
            tracing::warn!(error = %e, "Invalid JSON in send request");
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    success: false,
                    error: format!("Invalid JSON: {}", e),
                }),
            )
        })?
    } else {
        let body_text = String::from_utf8_lossy(&body).to_string();
        SendRequest {
            title: None,
            subtitle: None,
            body: if body_text.is_empty() {
                None
            } else {
                Some(body_text)
            },
            launch_image: None,
            title_loc_key: None,
            title_loc_args: None,
            loc_key: None,
            loc_args: None,
            badge: None,
            sound: None,
            content_available: None,
            mutable_content: None,
            category: None,
            interruption_level: None,
            relevance_score: None,
            priority: None,
            collapse_id: None,
            expiration: None,
            data: None,
        }
    };

    tracing::debug!(
        title = ?req.title,
        body = ?req.body,
        interruption_level = ?req.interruption_level,
        relevance_score = ?req.relevance_score,
        "Parsed send request"
    );

    // Fetch all devices
    let devices: Vec<(String, String)> =
        sqlx::query_as("SELECT device_token, environment FROM devices")
            .fetch_all(&state.db)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Database error fetching devices");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        success: false,
                        error: format!("Database error: {}", e),
                    }),
                )
            })?;

    tracing::info!(device_count = devices.len(), "Found devices to notify");

    if devices.is_empty() {
        tracing::warn!("No devices registered, nothing to send");
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                success: false,
                error: "No devices registered".to_string(),
            }),
        ));
    }

    let apns_clients = state.apns.read().await;
    let payload_json = serde_json::to_string(&req.data).ok();

    let mut results = Vec::new();
    let mut sent = 0;
    let mut failed = 0;

    for (device_token, env_str) in devices {
        let environment = match Environment::try_from(env_str.as_str()) {
            Ok(env) => env,
            Err(_) => {
                tracing::error!(device_token = %device_token, env = %env_str, "Invalid environment in database");
                results.push(DeviceSendResult {
                    device_token,
                    success: false,
                    apns_id: None,
                    error: Some("Invalid environment in database".to_string()),
                });
                failed += 1;
                continue;
            }
        };

        tracing::debug!(device_token = %device_token, environment = %env_str, "Sending to device");

        match apns_clients
            .send_notification(&device_token, &req, environment)
            .await
        {
            Ok(apns_id) => {
                tracing::info!(device_token = %device_token, apns_id = %apns_id, "Push sent");
                // Record the push
                let _ = sqlx::query(
                    r#"
                    INSERT INTO pushes (device_token, apns_id, title, body, payload)
                    VALUES (?, ?, ?, ?, ?)
                    "#,
                )
                .bind(&device_token)
                .bind(&apns_id)
                .bind(&req.title)
                .bind(&req.body)
                .bind(&payload_json)
                .execute(&state.db)
                .await;

                results.push(DeviceSendResult {
                    device_token,
                    success: true,
                    apns_id: Some(apns_id),
                    error: None,
                });
                sent += 1;
            }
            Err(e) => {
                tracing::error!(device_token = %device_token, error = %e, "Push failed");
                results.push(DeviceSendResult {
                    device_token,
                    success: false,
                    apns_id: None,
                    error: Some(e.to_string()),
                });
                failed += 1;
            }
        }
    }

    tracing::info!(sent = sent, failed = failed, "Send complete");

    Ok(Json(SendResponse {
        success: sent > 0,
        sent,
        failed,
        results,
    }))
}

async fn get_stats(
    State(state): State<AppState>,
) -> Result<Json<StatsResponse>, (StatusCode, Json<ErrorResponse>)> {
    let total_devices: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM devices")
        .fetch_one(&state.db)
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

    let sandbox_devices: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM devices WHERE environment = 'sandbox'")
            .fetch_one(&state.db)
            .await
            .unwrap_or((0,));

    let production_devices: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM devices WHERE environment = 'production'")
            .fetch_one(&state.db)
            .await
            .unwrap_or((0,));

    let total_pushes: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM pushes")
        .fetch_one(&state.db)
        .await
        .unwrap_or((0,));

    Ok(Json(StatsResponse {
        total_devices: total_devices.0,
        sandbox_devices: sandbox_devices.0,
        production_devices: production_devices.0,
        total_pushes: total_pushes.0,
    }))
}

async fn get_pushes(
    State(state): State<AppState>,
    Query(query): Query<PushesQuery>,
) -> Result<Json<PushesResponse>, (StatusCode, Json<ErrorResponse>)> {
    tracing::debug!(installation_id = %query.installation_id, "Fetching pushes");

    let pushes: Vec<PushRecord> = sqlx::query_as(
        r#"
        SELECT p.id, p.device_token, p.apns_id, p.title, p.body, p.payload, p.sent_at
        FROM pushes p
        JOIN devices d ON p.device_token = d.device_token
        WHERE d.installation_id = ?
        ORDER BY p.sent_at DESC
        "#,
    )
    .bind(&query.installation_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "Database error fetching pushes");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                success: false,
                error: format!("Database error: {}", e),
            }),
        )
    })?;

    tracing::debug!(count = pushes.len(), "Returning pushes");

    Ok(Json(PushesResponse { pushes }))
}

async fn get_push_detail(
    State(state): State<AppState>,
    axum::extract::Path(push_id): axum::extract::Path<i64>,
) -> Result<Json<PushDetailRecord>, (StatusCode, Json<ErrorResponse>)> {
    tracing::debug!(push_id = push_id, "Fetching push detail");

    let push: Option<PushDetailRecord> = sqlx::query_as(
        r#"
        SELECT
            p.id, p.apns_id, p.title, p.body, p.payload, p.sent_at, p.device_token,
            d.device_name, d.device_type, d.environment
        FROM pushes p
        LEFT JOIN devices d ON p.device_token = d.device_token
        WHERE p.id = ?
        "#,
    )
    .bind(push_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(push_id = push_id, error = %e, "Database error fetching push detail");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                success: false,
                error: format!("Database error: {}", e),
            }),
        )
    })?;

    match push {
        Some(p) => Ok(Json(p)),
        None => {
            tracing::warn!(push_id = push_id, "Push not found");
            Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    success: false,
                    error: "Push not found".to_string(),
                }),
            ))
        }
    }
}

async fn init_db(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS devices (
            device_token TEXT PRIMARY KEY,
            installation_id TEXT,
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

    // Add installation_id column if it doesn't exist (migration for existing DBs)
    let _ = sqlx::query("ALTER TABLE devices ADD COLUMN installation_id TEXT")
        .execute(pool)
        .await;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS pushes (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            device_token TEXT NOT NULL,
            apns_id TEXT,
            title TEXT,
            body TEXT,
            payload TEXT,
            sent_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
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
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:data.db".to_string());
    tracing::info!(database_url = %database_url, "Connecting to database");

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    init_db(&pool).await?;
    tracing::info!("Database initialized");

    let apns_clients = ApnsClients::new()?;
    tracing::info!("APNs clients initialized");

    let state = AppState {
        db: pool,
        apns: Arc::new(RwLock::new(apns_clients)),
    };

    let app = Router::new()
        .route("/", get(|| async { format!("OK {}", env!("GIT_HASH")) }))
        .route("/stats", get(get_stats))
        .route("/pushes", get(get_pushes))
        .route("/pushes/:id", get(get_push_detail))
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
        assert_eq!(
            Environment::try_from("sandbox").unwrap(),
            Environment::Sandbox
        );
        assert_eq!(
            Environment::try_from("production").unwrap(),
            Environment::Production
        );
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
            "installation_id": "uuid-install-1",
            "environment": "sandbox",
            "device_name": "John's iPhone"
        }"#;
        let req: RegisterRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.device_token, "abc123");
        assert_eq!(req.installation_id, "uuid-install-1");
        assert_eq!(req.environment, Environment::Sandbox);
        assert_eq!(req.device_name, Some("John's iPhone".to_string()));
    }

    #[test]
    fn test_deserialize_send_request_simple() {
        let json = r#"{
            "title": "Hello",
            "body": "World"
        }"#;
        let req: SendRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.title, Some("Hello".to_string()));
        assert_eq!(req.body, Some("World".to_string()));
    }

    #[test]
    fn test_deserialize_send_request_with_sound() {
        let json = r#"{
            "sound": "default"
        }"#;
        let req: SendRequest = serde_json::from_str(json).unwrap();
        assert!(matches!(req.sound, Some(SoundConfig::Simple(s)) if s == "default"));

        let json = r#"{
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
    fn test_deserialize_send_request_with_interruption_level() {
        let json = r#"{
            "title": "Hello",
            "interruption_level": "time-sensitive",
            "relevance_score": 0.75
        }"#;
        let req: SendRequest = serde_json::from_str(json).unwrap();
        assert_eq!(
            req.interruption_level,
            Some("time-sensitive".to_string())
        );
        assert!((req.relevance_score.unwrap() - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn test_deserialize_send_request_with_data() {
        let json = r#"{
            "data": {"key": "value", "number": 42}
        }"#;
        let req: SendRequest = serde_json::from_str(json).unwrap();
        let data = req.data.unwrap();
        assert_eq!(data.get("key").unwrap(), "value");
        assert_eq!(data.get("number").unwrap(), 42);
    }

    #[test]
    fn test_serialize_pushes_response() {
        let pushes = vec![
            PushRecord {
                id: 1,
                device_token: "abc123".to_string(),
                apns_id: Some("uuid-1".to_string()),
                title: Some("Test Title".to_string()),
                body: Some("Test Body".to_string()),
                payload: None,
                sent_at: "2024-01-01 12:00:00".to_string(),
            },
            PushRecord {
                id: 2,
                device_token: "def456".to_string(),
                apns_id: None,
                title: None,
                body: Some("Body only".to_string()),
                payload: Some(r#"{"key":"value"}"#.to_string()),
                sent_at: "2024-01-02 12:00:00".to_string(),
            },
        ];
        let response = PushesResponse { pushes };
        let json = serde_json::to_string(&response).unwrap();

        assert!(json.contains("\"id\":1"));
        assert!(json.contains("\"device_token\":\"abc123\""));
        assert!(json.contains("\"apns_id\":\"uuid-1\""));
        assert!(json.contains("\"title\":\"Test Title\""));
        assert!(json.contains("\"body\":\"Test Body\""));
        assert!(json.contains("\"sent_at\":\"2024-01-01 12:00:00\""));
        assert!(json.contains("\"id\":2"));
        assert!(json.contains("\"apns_id\":null"));
        assert!(json.contains("\"title\":null"));
    }

    #[test]
    fn test_deserialize_pushes_query() {
        let json = r#"{"installation_id": "uuid-install-1"}"#;
        let parsed: PushesQuery = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.installation_id, "uuid-install-1");
    }

    #[test]
    fn test_serialize_push_detail_record() {
        let detail = PushDetailRecord {
            id: 1,
            apns_id: Some("apns-uuid-1".to_string()),
            title: Some("Test Title".to_string()),
            body: Some("Test Body".to_string()),
            payload: Some(r#"{"key":"value"}"#.to_string()),
            sent_at: "2024-01-01 12:00:00".to_string(),
            device_token: "abc123".to_string(),
            device_name: Some("John's iPhone".to_string()),
            device_type: Some("iPhone".to_string()),
            environment: Some("sandbox".to_string()),
        };
        let json = serde_json::to_string(&detail).unwrap();

        assert!(json.contains("\"id\":1"));
        assert!(json.contains("\"apns_id\":\"apns-uuid-1\""));
        assert!(json.contains("\"device_name\":\"John's iPhone\""));
        assert!(json.contains("\"device_type\":\"iPhone\""));
        assert!(json.contains("\"environment\":\"sandbox\""));
    }
}
