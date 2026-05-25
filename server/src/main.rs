use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{header::CONTENT_TYPE, HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use seekwel::{connection::Connection, error::Error as SeekwelError, rusqlite::params};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, env, sync::Arc};
use tokio::sync::RwLock;

mod apns;

use apns::ApnsClients;

#[derive(Clone)]
struct AppState {
    apns: Arc<RwLock<ApnsClients>>,
}

struct Database;

#[derive(Debug, Clone, PartialEq, Eq)]
enum DatabaseLocation {
    Memory,
    File(String),
}

#[derive(Debug)]
struct DeviceTarget {
    id: i64,
    device_token: String,
    environment: String,
}

impl Database {
    fn initialize(database_url: &str) -> Result<(), SeekwelError> {
        match Self::location_from_url(database_url) {
            DatabaseLocation::Memory => match Connection::memory() {
                Ok(()) | Err(SeekwelError::AlreadyInitialized) => {}
                Err(error) => return Err(error),
            },
            DatabaseLocation::File(path) => match Connection::file(&path) {
                Ok(()) | Err(SeekwelError::AlreadyInitialized) => {}
                Err(error) => return Err(error),
            },
        }

        let conn = Connection::get()?;
        conn.execute("PRAGMA foreign_keys = ON", ())?;
        Connection::transaction(|| {
            Self::migrate_devices(&conn)?;
            Self::migrate_pushes(&conn)?;
            Self::create_schema(&conn)
        })
    }

    fn location_from_url(database_url: &str) -> DatabaseLocation {
        let mut value = database_url
            .strip_prefix("sqlite:")
            .unwrap_or(database_url)
            .split('?')
            .next()
            .unwrap_or(database_url);
        if let Some(path) = value.strip_prefix("//") {
            value = path;
        }

        if value == ":memory:" {
            DatabaseLocation::Memory
        } else {
            DatabaseLocation::File(value.to_string())
        }
    }

    fn create_schema(conn: &Connection) -> Result<(), SeekwelError> {
        Self::create_devices_table(conn)?;
        Self::create_pushes_table(conn)?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_devices_installation_id ON devices(installation_id)",
            (),
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_pushes_device_id_sent_at ON pushes(device_id, sent_at DESC)",
            (),
        )?;
        Ok(())
    }

    fn create_devices_table(conn: &Connection) -> Result<(), SeekwelError> {
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS devices (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                device_token TEXT NOT NULL UNIQUE,
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
            (),
        )?;
        Ok(())
    }

    fn create_pushes_table(conn: &Connection) -> Result<(), SeekwelError> {
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS pushes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                device_id INTEGER NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
                apns_id TEXT,
                title TEXT,
                body TEXT,
                payload TEXT,
                interruption_level TEXT,
                sent_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            )
            "#,
            (),
        )?;
        Ok(())
    }

    fn migrate_devices(conn: &Connection) -> Result<(), SeekwelError> {
        if !Self::table_exists(conn, "devices")? {
            return Self::create_devices_table(conn);
        }

        if Self::column_exists(conn, "devices", "id")? {
            return Ok(());
        }

        let legacy_columns = [
            "installation_id",
            "device_name",
            "device_type",
            "os_version",
            "app_version",
            "created_at",
            "updated_at",
        ];
        for column in legacy_columns {
            if !Self::column_exists(conn, "devices", column)? {
                let _ = conn.execute(&format!("ALTER TABLE devices ADD COLUMN {column} TEXT"), ());
            }
        }

        conn.execute("DROP TABLE IF EXISTS devices_old", ())?;
        conn.execute("ALTER TABLE devices RENAME TO devices_old", ())?;
        Self::create_devices_table(conn)?;
        conn.execute(
            r#"
            INSERT OR IGNORE INTO devices (
                device_token,
                installation_id,
                environment,
                device_name,
                device_type,
                os_version,
                app_version,
                created_at,
                updated_at
            )
            SELECT
                device_token,
                installation_id,
                environment,
                device_name,
                device_type,
                os_version,
                app_version,
                COALESCE(created_at, CURRENT_TIMESTAMP),
                COALESCE(updated_at, CURRENT_TIMESTAMP)
            FROM devices_old
            WHERE device_token IS NOT NULL
              AND environment IN ('sandbox', 'production')
            "#,
            (),
        )?;
        conn.execute("DROP TABLE devices_old", ())?;
        Ok(())
    }

    fn migrate_pushes(conn: &Connection) -> Result<(), SeekwelError> {
        if Self::table_exists(conn, "pushes")? && !Self::column_exists(conn, "pushes", "device_id")?
        {
            conn.execute("DROP TABLE pushes", ())?;
        }
        Self::create_pushes_table(conn)
    }

    fn table_exists(conn: &Connection, table: &str) -> Result<bool, SeekwelError> {
        let exists: i64 = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            params![table],
            |row| row.get(0),
        )?;
        Ok(exists != 0)
    }

    fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool, SeekwelError> {
        let sql = format!("PRAGMA table_info({table})");
        let columns: Vec<String> = conn.query_all(&sql, (), |row| row.get(1))?;
        Ok(columns.iter().any(|name| name == column))
    }

    fn upsert_device(req: &RegisterRequest) -> Result<(), SeekwelError> {
        Connection::get()?.execute(
            r#"
            INSERT INTO devices (
                device_token,
                installation_id,
                environment,
                device_name,
                device_type,
                os_version,
                app_version,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, CURRENT_TIMESTAMP)
            ON CONFLICT(device_token) DO UPDATE SET
                installation_id = excluded.installation_id,
                environment = excluded.environment,
                device_name = excluded.device_name,
                device_type = excluded.device_type,
                os_version = excluded.os_version,
                app_version = excluded.app_version,
                updated_at = CURRENT_TIMESTAMP
            "#,
            params![
                req.device_token,
                req.installation_id,
                req.environment.as_str(),
                req.device_name,
                req.device_type,
                req.os_version,
                req.app_version
            ],
        )?;
        Ok(())
    }

    fn delivery_targets() -> Result<Vec<DeviceTarget>, SeekwelError> {
        Connection::get()?.query_all(
            "SELECT id, device_token, environment FROM devices ORDER BY id",
            (),
            |row| {
                Ok(DeviceTarget {
                    id: row.get(0)?,
                    device_token: row.get(1)?,
                    environment: row.get(2)?,
                })
            },
        )
    }

    fn record_push(
        device_id: i64,
        apns_id: &str,
        req: &SendRequest,
        payload_json: Option<&str>,
    ) -> Result<(), SeekwelError> {
        Connection::get()?.execute(
            r#"
            INSERT INTO pushes (device_id, apns_id, title, body, payload, interruption_level)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                device_id,
                apns_id,
                req.title.as_deref(),
                req.body.as_deref(),
                payload_json,
                req.interruption_level.as_deref()
            ],
        )?;
        Ok(())
    }

    fn stats() -> Result<StatsResponse, SeekwelError> {
        let conn = Connection::get()?;
        let total_devices = Self::count(&conn, "SELECT COUNT(*) FROM devices")?;
        let sandbox_devices = Self::count(
            &conn,
            "SELECT COUNT(*) FROM devices WHERE environment = 'sandbox'",
        )?;
        let production_devices = Self::count(
            &conn,
            "SELECT COUNT(*) FROM devices WHERE environment = 'production'",
        )?;
        let total_pushes = Self::count(&conn, "SELECT COUNT(*) FROM pushes")?;

        Ok(StatsResponse {
            total_devices,
            sandbox_devices,
            production_devices,
            total_pushes,
        })
    }

    fn count(conn: &Connection, sql: &str) -> Result<i64, SeekwelError> {
        conn.query_row(sql, (), |row| row.get(0))
    }

    fn pushes_for_installation(installation_id: &str) -> Result<Vec<PushRecord>, SeekwelError> {
        Connection::get()?.query_all(
            r#"
            SELECT
                p.id,
                d.device_token,
                p.apns_id,
                p.title,
                p.body,
                p.payload,
                p.interruption_level,
                p.sent_at
            FROM pushes p
            JOIN devices d ON p.device_id = d.id
            WHERE d.installation_id = ?1
            ORDER BY p.sent_at DESC
            "#,
            params![installation_id],
            |row| {
                Ok(PushRecord {
                    id: row.get(0)?,
                    device_token: row.get(1)?,
                    apns_id: row.get(2)?,
                    title: row.get(3)?,
                    body: row.get(4)?,
                    payload: row.get(5)?,
                    interruption_level: row.get(6)?,
                    sent_at: row.get(7)?,
                })
            },
        )
    }

    fn push_detail(push_id: i64) -> Result<Option<PushDetailRecord>, SeekwelError> {
        Connection::get()?.query_optional(
            r#"
            SELECT
                p.id,
                p.apns_id,
                p.title,
                p.body,
                p.payload,
                p.interruption_level,
                p.sent_at,
                d.device_token,
                d.device_name,
                d.device_type,
                d.environment
            FROM pushes p
            JOIN devices d ON p.device_id = d.id
            WHERE p.id = ?1
            "#,
            params![push_id],
            |row| {
                Ok(PushDetailRecord {
                    id: row.get(0)?,
                    apns_id: row.get(1)?,
                    title: row.get(2)?,
                    body: row.get(3)?,
                    payload: row.get(4)?,
                    interruption_level: row.get(5)?,
                    sent_at: row.get(6)?,
                    device_token: row.get(7)?,
                    device_name: row.get(8)?,
                    device_type: row.get(9)?,
                    environment: row.get(10)?,
                })
            },
        )
    }
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

impl ErrorResponse {
    fn with_status(status: StatusCode, error: impl Into<String>) -> (StatusCode, Json<Self>) {
        (
            status,
            Json(Self {
                success: false,
                error: error.into(),
            }),
        )
    }
}

#[derive(Debug, Serialize)]
struct StatsResponse {
    total_devices: i64,
    sandbox_devices: i64,
    production_devices: i64,
    total_pushes: i64,
}

#[derive(Debug, Serialize)]
struct PushRecord {
    id: i64,
    device_token: String,
    apns_id: Option<String>,
    title: Option<String>,
    body: Option<String>,
    payload: Option<String>,
    interruption_level: Option<String>,
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

#[derive(Debug, Serialize)]
struct PushDetailRecord {
    id: i64,
    apns_id: Option<String>,
    title: Option<String>,
    body: Option<String>,
    payload: Option<String>,
    interruption_level: Option<String>,
    sent_at: String,
    device_token: String,
    device_name: Option<String>,
    device_type: Option<String>,
    environment: Option<String>,
}

async fn register_device(
    State(_state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> Result<Json<RegisterResponse>, (StatusCode, Json<ErrorResponse>)> {
    tracing::info!(
        device_token = %req.device_token,
        installation_id = %req.installation_id,
        environment = %req.environment.as_str(),
        device_name = ?req.device_name,
        "Registering device"
    );

    match Database::upsert_device(&req) {
        Ok(()) => {
            tracing::info!(device_token = %req.device_token, "Device registered");
            Ok(Json(RegisterResponse {
                success: true,
                message: "Device registered successfully".to_string(),
            }))
        }
        Err(e) => {
            tracing::error!(device_token = %req.device_token, error = %e, "Failed to register device");
            Err(ErrorResponse::with_status(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to register device: {e}"),
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

    tracing::info!(
        is_json = is_json,
        body_len = body.len(),
        "Received send request"
    );

    let req: SendRequest = if is_json {
        serde_json::from_slice(&body).map_err(|e| {
            tracing::warn!(error = %e, "Invalid JSON in send request");
            ErrorResponse::with_status(StatusCode::BAD_REQUEST, format!("Invalid JSON: {e}"))
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

    let devices = Database::delivery_targets().map_err(|e| {
        tracing::error!(error = %e, "Database error fetching devices");
        ErrorResponse::with_status(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Database error: {e}"),
        )
    })?;

    tracing::info!(device_count = devices.len(), "Found devices to notify");

    if devices.is_empty() {
        tracing::warn!("No devices registered, nothing to send");
        return Err(ErrorResponse::with_status(
            StatusCode::NOT_FOUND,
            "No devices registered",
        ));
    }

    let apns_clients = state.apns.read().await;
    let payload_json = serde_json::to_string(&req.data).ok();

    let mut results = Vec::new();
    let mut sent = 0;
    let mut failed = 0;

    for device in devices {
        let environment = match Environment::try_from(device.environment.as_str()) {
            Ok(env) => env,
            Err(_) => {
                tracing::error!(device_token = %device.device_token, env = %device.environment, "Invalid environment in database");
                results.push(DeviceSendResult {
                    device_token: device.device_token,
                    success: false,
                    apns_id: None,
                    error: Some("Invalid environment in database".to_string()),
                });
                failed += 1;
                continue;
            }
        };

        tracing::debug!(device_token = %device.device_token, environment = %device.environment, "Sending to device");

        match apns_clients
            .send_notification(&device.device_token, &req, environment)
            .await
        {
            Ok(apns_id) => {
                tracing::info!(device_token = %device.device_token, apns_id = %apns_id, "Push sent");
                let record_result =
                    Database::record_push(device.id, &apns_id, &req, payload_json.as_deref());

                if let Err(e) = record_result {
                    tracing::error!(device_token = %device.device_token, apns_id = %apns_id, error = %e, "Failed to record push");
                }

                results.push(DeviceSendResult {
                    device_token: device.device_token,
                    success: true,
                    apns_id: Some(apns_id),
                    error: None,
                });
                sent += 1;
            }
            Err(e) => {
                tracing::error!(device_token = %device.device_token, error = %e, "Push failed");
                results.push(DeviceSendResult {
                    device_token: device.device_token,
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
    State(_state): State<AppState>,
) -> Result<Json<StatsResponse>, (StatusCode, Json<ErrorResponse>)> {
    Database::stats().map(Json).map_err(|e| {
        ErrorResponse::with_status(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Database error: {e}"),
        )
    })
}

async fn get_pushes(
    State(_state): State<AppState>,
    Query(query): Query<PushesQuery>,
) -> Result<Json<PushesResponse>, (StatusCode, Json<ErrorResponse>)> {
    tracing::debug!(installation_id = %query.installation_id, "Fetching pushes");

    let pushes = Database::pushes_for_installation(&query.installation_id).map_err(|e| {
        tracing::error!(error = %e, "Database error fetching pushes");
        ErrorResponse::with_status(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Database error: {e}"),
        )
    })?;

    tracing::debug!(count = pushes.len(), "Returning pushes");

    Ok(Json(PushesResponse { pushes }))
}

async fn get_push_detail(
    State(_state): State<AppState>,
    Path(push_id): Path<i64>,
) -> Result<Json<PushDetailRecord>, (StatusCode, Json<ErrorResponse>)> {
    tracing::debug!(push_id = push_id, "Fetching push detail");

    let push = Database::push_detail(push_id).map_err(|e| {
        tracing::error!(push_id = push_id, error = %e, "Database error fetching push detail");
        ErrorResponse::with_status(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Database error: {e}"),
        )
    })?;

    match push {
        Some(p) => Ok(Json(p)),
        None => {
            tracing::warn!(push_id = push_id, "Push not found");
            Err(ErrorResponse::with_status(
                StatusCode::NOT_FOUND,
                "Push not found",
            ))
        }
    }
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

    Database::initialize(&database_url)?;
    tracing::info!("Database initialized");

    let apns_clients = ApnsClients::new()?;
    tracing::info!("APNs clients initialized");

    let state = AppState {
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
    fn test_database_location_from_url() {
        assert_eq!(
            Database::location_from_url("sqlite:data.db"),
            DatabaseLocation::File("data.db".to_string())
        );
        assert_eq!(
            Database::location_from_url("sqlite:/app/data/data.db?mode=rwc"),
            DatabaseLocation::File("/app/data/data.db".to_string())
        );
        assert_eq!(
            Database::location_from_url("sqlite:///app/data/data.db?mode=rwc"),
            DatabaseLocation::File("/app/data/data.db".to_string())
        );
        assert_eq!(
            Database::location_from_url("sqlite::memory:"),
            DatabaseLocation::Memory
        );
        assert_eq!(
            Database::location_from_url("/tmp/psh.db"),
            DatabaseLocation::File("/tmp/psh.db".to_string())
        );
    }

    #[test]
    fn test_migrates_legacy_devices_and_recreates_pushes() -> Result<(), SeekwelError> {
        match Connection::memory() {
            Ok(()) | Err(SeekwelError::AlreadyInitialized) => {}
            Err(error) => return Err(error),
        }

        let conn = Connection::get()?;
        conn.execute("DROP TABLE IF EXISTS pushes", ())?;
        conn.execute("DROP TABLE IF EXISTS devices", ())?;
        conn.execute("DROP TABLE IF EXISTS devices_old", ())?;
        conn.execute(
            r#"
            CREATE TABLE devices (
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
            (),
        )?;
        conn.execute(
            r#"
            INSERT INTO devices (
                device_token,
                installation_id,
                environment,
                device_name,
                device_type,
                os_version,
                app_version
            ) VALUES ('token-1', 'install-1', 'sandbox', 'Phone', 'iPhone', 'iOS', '1.0')
            "#,
            (),
        )?;
        conn.execute(
            r#"
            CREATE TABLE pushes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                device_token TEXT NOT NULL,
                apns_id TEXT,
                title TEXT,
                body TEXT,
                payload TEXT,
                sent_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            )
            "#,
            (),
        )?;
        conn.execute(
            "INSERT INTO pushes (device_token, title) VALUES ('token-1', 'old push')",
            (),
        )?;

        Database::migrate_devices(&conn)?;
        Database::migrate_pushes(&conn)?;
        Database::create_schema(&conn)?;

        assert!(Database::column_exists(&conn, "devices", "id")?);
        assert!(Database::column_exists(&conn, "pushes", "device_id")?);

        let token: String = conn.query_row(
            "SELECT device_token FROM devices WHERE installation_id = 'install-1'",
            (),
            |row| row.get(0),
        )?;
        assert_eq!(token, "token-1");

        let push_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM pushes", (), |row| row.get(0))?;
        assert_eq!(push_count, 0);

        Ok(())
    }

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
        assert_eq!(req.interruption_level, Some("time-sensitive".to_string()));
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
                interruption_level: None,
                sent_at: "2024-01-01 12:00:00".to_string(),
            },
            PushRecord {
                id: 2,
                device_token: "def456".to_string(),
                apns_id: None,
                title: None,
                body: Some("Body only".to_string()),
                payload: Some(r#"{"key":"value"}"#.to_string()),
                interruption_level: Some("time-sensitive".to_string()),
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
            interruption_level: Some("time-sensitive".to_string()),
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
