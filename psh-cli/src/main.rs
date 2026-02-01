use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{self, Write};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "psh")]
#[command(about = "Push notification server client")]
struct Cli {
    /// Server URL (required via flag, PSH_SERVER env, or config file)
    #[arg(short, long, env = "PSH_SERVER")]
    server: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct Config {
    server: Option<String>,
}

impl Config {
    fn load() -> Self {
        Self::config_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save(&self) -> Result<()> {
        let path = Self::config_path().context("Could not determine config directory")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|p| p.join("psh").join("config.toml"))
    }
}

fn prompt_for_server() -> Result<String> {
    print!("Server URL: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let server = input.trim().to_string();
    if server.is_empty() {
        anyhow::bail!("Server URL is required");
    }
    Ok(server)
}

fn resolve_server(cli_server: Option<String>, config: &Config) -> Result<String> {
    if let Some(server) = cli_server.or_else(|| config.server.clone()) {
        return Ok(server);
    }

    println!("No server configured.");
    let server = prompt_for_server()?;

    let mut config = Config::load();
    config.server = Some(server.clone());
    config.save()?;
    println!("Saved to {:?}", Config::config_path().unwrap());

    Ok(server)
}

#[derive(Subcommand)]
enum Commands {
    /// Send a push notification
    Send(SendArgs),
    /// Get server statistics
    Stats,
    /// Health check
    Ping,
}

#[derive(Parser)]
struct SendArgs {
    /// Notification body (positional)
    body_positional: Option<String>,

    // Alert options
    /// Notification title
    #[arg(short, long)]
    title: Option<String>,

    /// Notification subtitle
    #[arg(long)]
    subtitle: Option<String>,

    /// Notification body (alternative to positional)
    #[arg(short, long)]
    body: Option<String>,

    /// Launch image name
    #[arg(long)]
    launch_image: Option<String>,

    // Localization
    /// Localization key for title
    #[arg(long)]
    title_loc_key: Option<String>,

    /// Comma-separated args for title localization
    #[arg(long)]
    title_loc_args: Option<String>,

    /// Localization key for body
    #[arg(long)]
    loc_key: Option<String>,

    /// Comma-separated args for body localization
    #[arg(long)]
    loc_args: Option<String>,

    // Badge & Sound
    /// Badge count
    #[arg(long)]
    badge: Option<u32>,

    /// Simple sound name (e.g., "default")
    #[arg(long)]
    sound: Option<String>,

    /// Use critical alert sound
    #[arg(long)]
    sound_critical: bool,

    /// Sound name for critical alert
    #[arg(long)]
    sound_name: Option<String>,

    /// Sound volume (0.0-1.0) for critical alert
    #[arg(long)]
    sound_volume: Option<f64>,

    // Behavior
    /// Silent/background notification
    #[arg(long)]
    content_available: bool,

    /// Allow extension modification
    #[arg(long)]
    mutable_content: bool,

    /// Notification category
    #[arg(long)]
    category: Option<String>,

    // Delivery options
    /// Priority (1-10, 10 = highest)
    #[arg(long)]
    priority: Option<u8>,

    /// Collapse identifier
    #[arg(long)]
    collapse_id: Option<String>,

    /// Expiration unix timestamp
    #[arg(long)]
    expiration: Option<u64>,

    // Custom data
    /// Custom key=value pairs (repeatable)
    #[arg(short = 'd', long = "data")]
    data: Vec<String>,
}

#[derive(Serialize)]
struct SendRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    subtitle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    launch_image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title_loc_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title_loc_args: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    loc_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    loc_args: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    badge: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sound: Option<SoundConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_available: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mutable_content: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    priority: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    collapse_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expiration: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<HashMap<String, Value>>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum SoundConfig {
    Simple(String),
    Critical {
        name: String,
        critical: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        volume: Option<f64>,
    },
}

#[derive(Deserialize)]
struct SendResponse {
    #[allow(dead_code)]
    success: bool,
    sent: usize,
    failed: usize,
    results: Vec<DeviceSendResult>,
}

#[derive(Deserialize)]
struct DeviceSendResult {
    device_token: String,
    success: bool,
    apns_id: Option<String>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct StatsResponse {
    total_devices: i64,
    sandbox_devices: i64,
    production_devices: i64,
    total_pushes: i64,
}

#[derive(Deserialize)]
struct ErrorResponse {
    error: String,
}

impl SendArgs {
    fn into_request(self) -> SendRequest {
        let body = self.body.or(self.body_positional);

        let title_loc_args = self
            .title_loc_args
            .map(|s| s.split(',').map(|s| s.trim().to_string()).collect());

        let loc_args = self
            .loc_args
            .map(|s| s.split(',').map(|s| s.trim().to_string()).collect());

        let sound = if self.sound_critical {
            Some(SoundConfig::Critical {
                name: self.sound_name.unwrap_or_else(|| "default".to_string()),
                critical: true,
                volume: self.sound_volume,
            })
        } else {
            self.sound.map(SoundConfig::Simple)
        };

        let data = if self.data.is_empty() {
            None
        } else {
            let mut map = HashMap::new();
            for pair in self.data {
                if let Some((key, value)) = pair.split_once('=') {
                    map.insert(key.to_string(), Value::String(value.to_string()));
                }
            }
            Some(map)
        };

        let content_available = if self.content_available {
            Some(true)
        } else {
            None
        };

        let mutable_content = if self.mutable_content {
            Some(true)
        } else {
            None
        };

        SendRequest {
            title: self.title,
            subtitle: self.subtitle,
            body,
            launch_image: self.launch_image,
            title_loc_key: self.title_loc_key,
            title_loc_args,
            loc_key: self.loc_key,
            loc_args,
            badge: self.badge,
            sound,
            content_available,
            mutable_content,
            category: self.category,
            priority: self.priority,
            collapse_id: self.collapse_id,
            expiration: self.expiration,
            data,
        }
    }
}

async fn cmd_send(server: &str, args: SendArgs) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/send", server);
    let request = args.into_request();

    let response = client
        .post(&url)
        .json(&request)
        .send()
        .await
        .context("Failed to connect to server")?;

    let status = response.status();
    if status.is_success() {
        let result: SendResponse = response.json().await.context("Invalid response")?;
        println!(
            "Sent: {}, Failed: {}",
            result.sent, result.failed
        );
        for r in result.results {
            if r.success {
                println!(
                    "  {} -> {}",
                    truncate_token(&r.device_token),
                    r.apns_id.unwrap_or_default()
                );
            } else {
                println!(
                    "  {} -> ERROR: {}",
                    truncate_token(&r.device_token),
                    r.error.unwrap_or_else(|| "Unknown error".to_string())
                );
            }
        }
    } else {
        let error: ErrorResponse = response
            .json()
            .await
            .unwrap_or(ErrorResponse {
                error: format!("HTTP {}", status),
            });
        anyhow::bail!("Error: {}", error.error);
    }

    Ok(())
}

async fn cmd_stats(server: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/stats", server);

    let response = client
        .get(&url)
        .send()
        .await
        .context("Failed to connect to server")?;

    let status = response.status();
    if status.is_success() {
        let stats: StatsResponse = response.json().await.context("Invalid response")?;
        println!("Devices: {} total ({} sandbox, {} production)",
            stats.total_devices,
            stats.sandbox_devices,
            stats.production_devices
        );
        println!("Pushes: {}", stats.total_pushes);
    } else {
        let error: ErrorResponse = response
            .json()
            .await
            .unwrap_or(ErrorResponse {
                error: format!("HTTP {}", status),
            });
        anyhow::bail!("Error: {}", error.error);
    }

    Ok(())
}

async fn cmd_ping(server: &str) -> Result<()> {
    let client = reqwest::Client::new();

    let response = client
        .get(server)
        .send()
        .await
        .context("Failed to connect to server")?;

    if response.status().is_success() {
        println!("Server is healthy");
    } else {
        anyhow::bail!("Server returned status: {}", response.status());
    }

    Ok(())
}

fn truncate_token(token: &str) -> String {
    if token.len() > 16 {
        format!("{}...{}", &token[..8], &token[token.len() - 8..])
    } else {
        token.to_string()
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = Config::load();
    let server = resolve_server(cli.server, &config)?;

    match cli.command {
        Commands::Send(args) => cmd_send(&server, args).await,
        Commands::Stats => cmd_stats(&server).await,
        Commands::Ping => cmd_ping(&server).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_send_args_basic_body() {
        let args = SendArgs {
            body_positional: Some("Hello".to_string()),
            title: None,
            subtitle: None,
            body: None,
            launch_image: None,
            title_loc_key: None,
            title_loc_args: None,
            loc_key: None,
            loc_args: None,
            badge: None,
            sound: None,
            sound_critical: false,
            sound_name: None,
            sound_volume: None,
            content_available: false,
            mutable_content: false,
            category: None,
            priority: None,
            collapse_id: None,
            expiration: None,
            data: vec![],
        };
        let req = args.into_request();
        assert_eq!(req.body, Some("Hello".to_string()));
        assert!(req.title.is_none());
    }

    #[test]
    fn test_send_args_body_flag_overrides_positional() {
        let args = SendArgs {
            body_positional: Some("Positional".to_string()),
            title: None,
            subtitle: None,
            body: Some("Flag".to_string()),
            launch_image: None,
            title_loc_key: None,
            title_loc_args: None,
            loc_key: None,
            loc_args: None,
            badge: None,
            sound: None,
            sound_critical: false,
            sound_name: None,
            sound_volume: None,
            content_available: false,
            mutable_content: false,
            category: None,
            priority: None,
            collapse_id: None,
            expiration: None,
            data: vec![],
        };
        let req = args.into_request();
        assert_eq!(req.body, Some("Flag".to_string()));
    }

    #[test]
    fn test_send_args_with_title() {
        let args = SendArgs {
            body_positional: Some("Body".to_string()),
            title: Some("Title".to_string()),
            subtitle: None,
            body: None,
            launch_image: None,
            title_loc_key: None,
            title_loc_args: None,
            loc_key: None,
            loc_args: None,
            badge: None,
            sound: None,
            sound_critical: false,
            sound_name: None,
            sound_volume: None,
            content_available: false,
            mutable_content: false,
            category: None,
            priority: None,
            collapse_id: None,
            expiration: None,
            data: vec![],
        };
        let req = args.into_request();
        assert_eq!(req.title, Some("Title".to_string()));
        assert_eq!(req.body, Some("Body".to_string()));
    }

    #[test]
    fn test_send_args_simple_sound() {
        let args = SendArgs {
            body_positional: None,
            title: None,
            subtitle: None,
            body: None,
            launch_image: None,
            title_loc_key: None,
            title_loc_args: None,
            loc_key: None,
            loc_args: None,
            badge: None,
            sound: Some("default".to_string()),
            sound_critical: false,
            sound_name: None,
            sound_volume: None,
            content_available: false,
            mutable_content: false,
            category: None,
            priority: None,
            collapse_id: None,
            expiration: None,
            data: vec![],
        };
        let req = args.into_request();
        assert!(matches!(req.sound, Some(SoundConfig::Simple(s)) if s == "default"));
    }

    #[test]
    fn test_send_args_critical_sound() {
        let args = SendArgs {
            body_positional: None,
            title: None,
            subtitle: None,
            body: None,
            launch_image: None,
            title_loc_key: None,
            title_loc_args: None,
            loc_key: None,
            loc_args: None,
            badge: None,
            sound: None,
            sound_critical: true,
            sound_name: Some("alert.caf".to_string()),
            sound_volume: Some(0.8),
            content_available: false,
            mutable_content: false,
            category: None,
            priority: None,
            collapse_id: None,
            expiration: None,
            data: vec![],
        };
        let req = args.into_request();
        match req.sound {
            Some(SoundConfig::Critical { name, critical, volume }) => {
                assert_eq!(name, "alert.caf");
                assert!(critical);
                assert_eq!(volume, Some(0.8));
            }
            _ => panic!("Expected critical sound config"),
        }
    }

    #[test]
    fn test_send_args_critical_sound_default_name() {
        let args = SendArgs {
            body_positional: None,
            title: None,
            subtitle: None,
            body: None,
            launch_image: None,
            title_loc_key: None,
            title_loc_args: None,
            loc_key: None,
            loc_args: None,
            badge: None,
            sound: None,
            sound_critical: true,
            sound_name: None,
            sound_volume: None,
            content_available: false,
            mutable_content: false,
            category: None,
            priority: None,
            collapse_id: None,
            expiration: None,
            data: vec![],
        };
        let req = args.into_request();
        match req.sound {
            Some(SoundConfig::Critical { name, .. }) => {
                assert_eq!(name, "default");
            }
            _ => panic!("Expected critical sound config"),
        }
    }

    #[test]
    fn test_send_args_data_parsing() {
        let args = SendArgs {
            body_positional: None,
            title: None,
            subtitle: None,
            body: None,
            launch_image: None,
            title_loc_key: None,
            title_loc_args: None,
            loc_key: None,
            loc_args: None,
            badge: None,
            sound: None,
            sound_critical: false,
            sound_name: None,
            sound_volume: None,
            content_available: false,
            mutable_content: false,
            category: None,
            priority: None,
            collapse_id: None,
            expiration: None,
            data: vec!["key1=value1".to_string(), "key2=value2".to_string()],
        };
        let req = args.into_request();
        let data = req.data.unwrap();
        assert_eq!(data.get("key1").unwrap(), "value1");
        assert_eq!(data.get("key2").unwrap(), "value2");
    }

    #[test]
    fn test_send_args_loc_args_parsing() {
        let args = SendArgs {
            body_positional: None,
            title: None,
            subtitle: None,
            body: None,
            launch_image: None,
            title_loc_key: Some("TITLE_KEY".to_string()),
            title_loc_args: Some("arg1, arg2, arg3".to_string()),
            loc_key: Some("BODY_KEY".to_string()),
            loc_args: Some("a,b".to_string()),
            badge: None,
            sound: None,
            sound_critical: false,
            sound_name: None,
            sound_volume: None,
            content_available: false,
            mutable_content: false,
            category: None,
            priority: None,
            collapse_id: None,
            expiration: None,
            data: vec![],
        };
        let req = args.into_request();
        assert_eq!(req.title_loc_key, Some("TITLE_KEY".to_string()));
        assert_eq!(
            req.title_loc_args,
            Some(vec!["arg1".to_string(), "arg2".to_string(), "arg3".to_string()])
        );
        assert_eq!(req.loc_key, Some("BODY_KEY".to_string()));
        assert_eq!(req.loc_args, Some(vec!["a".to_string(), "b".to_string()]));
    }

    #[test]
    fn test_send_args_content_available() {
        let args = SendArgs {
            body_positional: None,
            title: None,
            subtitle: None,
            body: None,
            launch_image: None,
            title_loc_key: None,
            title_loc_args: None,
            loc_key: None,
            loc_args: None,
            badge: None,
            sound: None,
            sound_critical: false,
            sound_name: None,
            sound_volume: None,
            content_available: true,
            mutable_content: false,
            category: None,
            priority: None,
            collapse_id: None,
            expiration: None,
            data: vec![],
        };
        let req = args.into_request();
        assert_eq!(req.content_available, Some(true));
        assert!(req.mutable_content.is_none());
    }

    #[test]
    fn test_send_args_mutable_content() {
        let args = SendArgs {
            body_positional: None,
            title: None,
            subtitle: None,
            body: None,
            launch_image: None,
            title_loc_key: None,
            title_loc_args: None,
            loc_key: None,
            loc_args: None,
            badge: None,
            sound: None,
            sound_critical: false,
            sound_name: None,
            sound_volume: None,
            content_available: false,
            mutable_content: true,
            category: None,
            priority: None,
            collapse_id: None,
            expiration: None,
            data: vec![],
        };
        let req = args.into_request();
        assert!(req.content_available.is_none());
        assert_eq!(req.mutable_content, Some(true));
    }

    #[test]
    fn test_send_request_serialization() {
        let req = SendRequest {
            title: Some("Test".to_string()),
            subtitle: None,
            body: Some("Body".to_string()),
            launch_image: None,
            title_loc_key: None,
            title_loc_args: None,
            loc_key: None,
            loc_args: None,
            badge: Some(5),
            sound: Some(SoundConfig::Simple("default".to_string())),
            content_available: None,
            mutable_content: None,
            category: None,
            priority: None,
            collapse_id: None,
            expiration: None,
            data: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"title\":\"Test\""));
        assert!(json.contains("\"body\":\"Body\""));
        assert!(json.contains("\"badge\":5"));
        assert!(json.contains("\"sound\":\"default\""));
        assert!(!json.contains("subtitle"));
        assert!(!json.contains("content_available"));
    }

    #[test]
    fn test_send_request_critical_sound_serialization() {
        let req = SendRequest {
            title: None,
            subtitle: None,
            body: None,
            launch_image: None,
            title_loc_key: None,
            title_loc_args: None,
            loc_key: None,
            loc_args: None,
            badge: None,
            sound: Some(SoundConfig::Critical {
                name: "alert.caf".to_string(),
                critical: true,
                volume: Some(0.5),
            }),
            content_available: None,
            mutable_content: None,
            category: None,
            priority: None,
            collapse_id: None,
            expiration: None,
            data: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"name\":\"alert.caf\""));
        assert!(json.contains("\"critical\":true"));
        assert!(json.contains("\"volume\":0.5"));
    }

    #[test]
    fn test_truncate_token_short() {
        assert_eq!(truncate_token("short"), "short");
        assert_eq!(truncate_token("exactly16chars!!"), "exactly16chars!!");
    }

    #[test]
    fn test_truncate_token_long() {
        let long_token = "abcdefghijklmnopqrstuvwxyz123456";
        let truncated = truncate_token(long_token);
        assert_eq!(truncated, "abcdefgh...yz123456");
    }

    #[test]
    fn test_truncate_token_exact_boundary() {
        let token_17 = "12345678901234567";
        let truncated = truncate_token(token_17);
        assert_eq!(truncated, "12345678...01234567");
    }

    #[test]
    fn test_config_parse() {
        let toml = r#"
server = "https://push.example.com"
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.server, Some("https://push.example.com".to_string()));
    }

    #[test]
    fn test_config_parse_empty() {
        let toml = "";
        let config: Config = toml::from_str(toml).unwrap();
        assert!(config.server.is_none());
    }

    #[test]
    fn test_resolve_server_cli_takes_priority() {
        let config = Config {
            server: Some("https://config.example.com".to_string()),
        };
        let result = resolve_server(Some("https://cli.example.com".to_string()), &config).unwrap();
        assert_eq!(result, "https://cli.example.com");
    }

    #[test]
    fn test_resolve_server_config_fallback() {
        let config = Config {
            server: Some("https://config.example.com".to_string()),
        };
        let result = resolve_server(None, &config).unwrap();
        assert_eq!(result, "https://config.example.com");
    }

    #[test]
    fn test_config_serialize() {
        let config = Config {
            server: Some("https://example.com".to_string()),
        };
        let toml = toml::to_string_pretty(&config).unwrap();
        assert!(toml.contains("server = \"https://example.com\""));
    }
}
