use a2::{
    request::payload::PayloadLike, Client, ClientConfig, CollapseId, Endpoint, NotificationOptions,
    Priority, PushType,
};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::env;
use std::fs::File;

use crate::{Environment, SendRequest, SoundConfig};

#[derive(Debug, Serialize)]
struct CustomPayload<'a> {
    aps: CustomAps,
    #[serde(flatten)]
    data: BTreeMap<String, Value>,
    #[serde(skip)]
    device_token: &'a str,
    #[serde(skip)]
    options: NotificationOptions<'a>,
}

impl<'a> PayloadLike for CustomPayload<'a> {
    fn get_device_token(&self) -> &str {
        self.device_token
    }

    fn get_options(&self) -> &NotificationOptions<'_> {
        &self.options
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
struct CustomAps {
    #[serde(skip_serializing_if = "Option::is_none")]
    alert: Option<CustomAlert>,
    #[serde(skip_serializing_if = "Option::is_none")]
    badge: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sound: Option<CustomSound>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_available: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mutable_content: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    interruption_level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    relevance_score: Option<f64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
struct CustomAlert {
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
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum CustomSound {
    Simple(String),
    Critical {
        critical: u8,
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        volume: Option<f64>,
    },
}

fn build_alert(req: &SendRequest) -> Option<CustomAlert> {
    let has_alert = req.title.is_some()
        || req.subtitle.is_some()
        || req.body.is_some()
        || req.launch_image.is_some()
        || req.title_loc_key.is_some()
        || req.loc_key.is_some();

    if !has_alert {
        return None;
    }

    Some(CustomAlert {
        title: req.title.clone(),
        subtitle: req.subtitle.clone(),
        body: req.body.clone(),
        launch_image: req.launch_image.clone(),
        title_loc_key: req.title_loc_key.clone(),
        title_loc_args: req.title_loc_args.clone(),
        loc_key: req.loc_key.clone(),
        loc_args: req.loc_args.clone(),
    })
}

fn build_sound(sound: &SoundConfig) -> CustomSound {
    match sound {
        SoundConfig::Simple(name) => CustomSound::Simple(name.clone()),
        SoundConfig::Critical {
            name,
            critical,
            volume,
        } => {
            if *critical == Some(true) {
                CustomSound::Critical {
                    critical: 1,
                    name: name.clone(),
                    volume: *volume,
                }
            } else {
                CustomSound::Simple(name.clone())
            }
        }
    }
}

fn build_custom_aps(req: &SendRequest) -> CustomAps {
    CustomAps {
        alert: build_alert(req),
        badge: req.badge,
        sound: req.sound.as_ref().map(build_sound),
        content_available: if req.content_available == Some(true) { Some(1) } else { None },
        mutable_content: if req.mutable_content == Some(true) { Some(1) } else { None },
        category: req.category.clone(),
        interruption_level: req.interruption_level.clone(),
        relevance_score: req.relevance_score,
    }
}

pub struct ApnsClients {
    sandbox: Client,
    production: Client,
    topic: String,
}

impl ApnsClients {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let key_path = env::var("APNS_KEY_PATH")?;
        let key_id = env::var("APNS_KEY_ID")?;
        let team_id = env::var("APNS_TEAM_ID")?;
        let topic = env::var("APNS_TOPIC")?;

        tracing::info!(key_path = %key_path, key_id = %key_id, team_id = %team_id, topic = %topic, "Configuring APNs clients");

        let mut key_file = File::open(&key_path)?;
        let sandbox_config = ClientConfig::new(Endpoint::Sandbox);
        let sandbox = Client::token(&mut key_file, &key_id, &team_id, sandbox_config)?;
        tracing::debug!("Sandbox client created");

        let mut key_file = File::open(&key_path)?;
        let production_config = ClientConfig::new(Endpoint::Production);
        let production = Client::token(&mut key_file, &key_id, &team_id, production_config)?;
        tracing::debug!("Production client created");

        Ok(Self {
            sandbox,
            production,
            topic,
        })
    }

    pub async fn send_notification(
        &self,
        device_token: &str,
        req: &SendRequest,
        environment: Environment,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let client = match environment {
            Environment::Sandbox => &self.sandbox,
            Environment::Production => &self.production,
        };

        let mut options = NotificationOptions {
            apns_topic: Some(&self.topic),
            ..Default::default()
        };

        if let Some(priority) = req.priority {
            options.apns_priority = Some(match priority {
                1..=5 => Priority::Normal,
                _ => Priority::High,
            });
        }

        if let Some(ref collapse_id) = req.collapse_id {
            if let Ok(cid) = CollapseId::new(collapse_id) {
                options.apns_collapse_id = Some(cid);
            }
        }

        if let Some(expiration) = req.expiration {
            options.apns_expiration = Some(expiration);
        }

        if req.content_available == Some(true) {
            options.apns_push_type = Some(PushType::Background);
        } else {
            options.apns_push_type = Some(PushType::Alert);
        }

        let data: BTreeMap<String, Value> = req
            .data
            .as_ref()
            .map(|d| d.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();

        let payload = CustomPayload {
            aps: build_custom_aps(req),
            data,
            device_token,
            options,
        };

        if let Ok(json) = payload.to_json_string() {
            tracing::debug!(device_token = %device_token, payload = %json, "Sending APNs payload");
        }

        let response = client.send(payload).await?;
        let apns_id = response.apns_id.unwrap_or_default();

        tracing::debug!(device_token = %device_token, apns_id = %apns_id, "APNs response received");

        Ok(apns_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_send_request() -> SendRequest {
        SendRequest {
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
    }

    fn build_test_payload(req: &SendRequest) -> String {
        let payload = CustomPayload {
            aps: build_custom_aps(req),
            data: BTreeMap::new(),
            device_token: "test_token",
            options: Default::default(),
        };
        payload.to_json_string().unwrap()
    }

    #[test]
    fn test_build_payload_with_title_and_body() {
        let mut req = make_send_request();
        req.title = Some("Test Title".to_string());
        req.body = Some("Test Body".to_string());

        let payload_str = build_test_payload(&req);

        assert!(payload_str.contains("Test Title"));
        assert!(payload_str.contains("Test Body"));
    }

    #[test]
    fn test_build_payload_with_badge() {
        let mut req = make_send_request();
        req.badge = Some(5);

        let payload_str = build_test_payload(&req);

        assert!(payload_str.contains("\"badge\":5"));
    }

    #[test]
    fn test_build_payload_with_simple_sound() {
        let mut req = make_send_request();
        req.sound = Some(SoundConfig::Simple("chime.caf".to_string()));

        let payload_str = build_test_payload(&req);

        assert!(payload_str.contains("chime.caf"));
    }

    #[test]
    fn test_build_payload_with_category() {
        let mut req = make_send_request();
        req.category = Some("MESSAGE".to_string());

        let payload_str = build_test_payload(&req);

        assert!(payload_str.contains("MESSAGE"));
    }

    #[test]
    fn test_build_payload_with_content_available() {
        let mut req = make_send_request();
        req.content_available = Some(true);

        let payload_str = build_test_payload(&req);

        assert!(payload_str.contains("\"content-available\":1"));
    }

    #[test]
    fn test_build_payload_with_mutable_content() {
        let mut req = make_send_request();
        req.mutable_content = Some(true);

        let payload_str = build_test_payload(&req);

        assert!(payload_str.contains("\"mutable-content\":1"));
    }

    #[test]
    fn test_build_payload_with_localization() {
        let mut req = make_send_request();
        req.title_loc_key = Some("TITLE_KEY".to_string());
        req.title_loc_args = Some(vec!["arg1".to_string(), "arg2".to_string()]);
        req.loc_key = Some("BODY_KEY".to_string());
        req.loc_args = Some(vec!["body_arg".to_string()]);

        let payload_str = build_test_payload(&req);

        assert!(payload_str.contains("TITLE_KEY"));
        assert!(payload_str.contains("BODY_KEY"));
    }

    #[test]
    fn test_build_payload_with_critical_sound() {
        let mut req = make_send_request();
        req.sound = Some(SoundConfig::Critical {
            name: "alert.caf".to_string(),
            critical: Some(true),
            volume: Some(0.8),
        });

        let payload_str = build_test_payload(&req);

        assert!(payload_str.contains("critical"));
    }

    #[test]
    fn test_build_payload_with_interruption_level() {
        let mut req = make_send_request();
        req.title = Some("Urgent".to_string());
        req.interruption_level = Some("time-sensitive".to_string());

        let payload_str = build_test_payload(&req);

        assert!(payload_str.contains("\"interruption-level\":\"time-sensitive\""));
    }

    #[test]
    fn test_build_payload_with_relevance_score() {
        let mut req = make_send_request();
        req.title = Some("Score".to_string());
        req.relevance_score = Some(0.75);

        let payload_str = build_test_payload(&req);

        assert!(payload_str.contains("\"relevance-score\":0.75"));
    }

    #[test]
    fn test_build_payload_without_interruption_level() {
        let mut req = make_send_request();
        req.title = Some("Normal".to_string());

        let payload_str = build_test_payload(&req);

        assert!(!payload_str.contains("interruption-level"));
        assert!(!payload_str.contains("relevance-score"));
    }
}
