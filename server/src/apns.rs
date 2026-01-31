use a2::{
    Client, ClientConfig, CollapseId, DefaultNotificationBuilder, Endpoint, NotificationBuilder,
    NotificationOptions, Priority, PushType,
};
use std::env;
use std::fs::File;

use crate::{Environment, SendRequest, SoundConfig};

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

        let mut key_file = File::open(&key_path)?;
        let sandbox_config = ClientConfig::new(Endpoint::Sandbox);
        let sandbox = Client::token(&mut key_file, &key_id, &team_id, sandbox_config)?;

        let mut key_file = File::open(&key_path)?;
        let production_config = ClientConfig::new(Endpoint::Production);
        let production = Client::token(&mut key_file, &key_id, &team_id, production_config)?;

        Ok(Self {
            sandbox,
            production,
            topic,
        })
    }

    pub async fn send_notification(
        &self,
        req: &SendRequest,
        environment: Environment,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let client = match environment {
            Environment::Sandbox => &self.sandbox,
            Environment::Production => &self.production,
        };

        // Pre-compute localization args so they live long enough
        let title_loc_args_refs: Option<Vec<&str>> = req
            .title_loc_args
            .as_ref()
            .map(|args| args.iter().map(|s| s.as_str()).collect());
        let loc_args_refs: Option<Vec<&str>> = req
            .loc_args
            .as_ref()
            .map(|args| args.iter().map(|s| s.as_str()).collect());

        let mut builder = DefaultNotificationBuilder::new();

        // Alert content
        if let Some(ref title) = req.title {
            builder = builder.set_title(title);
        }
        if let Some(ref subtitle) = req.subtitle {
            builder = builder.set_subtitle(subtitle);
        }
        if let Some(ref body) = req.body {
            builder = builder.set_body(body);
        }
        if let Some(ref launch_image) = req.launch_image {
            builder = builder.set_launch_image(launch_image);
        }

        // Localization
        if let Some(ref key) = req.title_loc_key {
            builder = builder.set_title_loc_key(key);
        }
        if let Some(ref args) = title_loc_args_refs {
            builder = builder.set_title_loc_args(args);
        }
        if let Some(ref key) = req.loc_key {
            builder = builder.set_loc_key(key);
        }
        if let Some(ref args) = loc_args_refs {
            builder = builder.set_loc_args(args);
        }

        // Badge
        if let Some(badge) = req.badge {
            builder = builder.set_badge(badge);
        }

        // Sound
        if let Some(ref sound) = req.sound {
            match sound {
                SoundConfig::Simple(name) => {
                    builder = builder.set_sound(name);
                }
                SoundConfig::Critical {
                    name,
                    critical,
                    volume,
                } => {
                    if *critical == Some(true) {
                        builder = builder.set_critical(true, *volume);
                    } else {
                        builder = builder.set_sound(name);
                    }
                }
            }
        }

        // Behavior flags
        if req.content_available == Some(true) {
            builder = builder.set_content_available();
        }
        if req.mutable_content == Some(true) {
            builder = builder.set_mutable_content();
        }
        if let Some(ref category) = req.category {
            builder = builder.set_category(category);
        }

        // Build notification options
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

        let mut payload = builder.build(&req.device_token, options);

        // Add custom data if present
        if let Some(ref data) = req.data {
            for (key, value) in data {
                payload.add_custom_data(key, value)?;
            }
        }

        let response = client.send(payload).await?;

        Ok(response.apns_id.unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use a2::request::payload::PayloadLike;

    fn make_send_request() -> SendRequest {
        SendRequest {
            device_token: "test_token".to_string(),
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
            priority: None,
            collapse_id: None,
            expiration: None,
            data: None,
        }
    }

    fn build_test_payload(req: &SendRequest) -> String {
        // Pre-compute localization args so they live long enough
        let title_loc_args_refs: Option<Vec<&str>> = req
            .title_loc_args
            .as_ref()
            .map(|args| args.iter().map(|s| s.as_str()).collect());
        let loc_args_refs: Option<Vec<&str>> = req
            .loc_args
            .as_ref()
            .map(|args| args.iter().map(|s| s.as_str()).collect());

        let mut builder = DefaultNotificationBuilder::new();

        if let Some(ref title) = req.title {
            builder = builder.set_title(title);
        }
        if let Some(ref subtitle) = req.subtitle {
            builder = builder.set_subtitle(subtitle);
        }
        if let Some(ref body) = req.body {
            builder = builder.set_body(body);
        }
        if let Some(ref launch_image) = req.launch_image {
            builder = builder.set_launch_image(launch_image);
        }
        if let Some(ref key) = req.title_loc_key {
            builder = builder.set_title_loc_key(key);
        }
        if let Some(ref args) = title_loc_args_refs {
            builder = builder.set_title_loc_args(args);
        }
        if let Some(ref key) = req.loc_key {
            builder = builder.set_loc_key(key);
        }
        if let Some(ref args) = loc_args_refs {
            builder = builder.set_loc_args(args);
        }
        if let Some(badge) = req.badge {
            builder = builder.set_badge(badge);
        }
        if let Some(ref sound) = req.sound {
            match sound {
                SoundConfig::Simple(name) => {
                    builder = builder.set_sound(name);
                }
                SoundConfig::Critical {
                    name,
                    critical,
                    volume,
                } => {
                    if *critical == Some(true) {
                        builder = builder.set_critical(true, *volume);
                    } else {
                        builder = builder.set_sound(name);
                    }
                }
            }
        }
        if req.content_available == Some(true) {
            builder = builder.set_content_available();
        }
        if req.mutable_content == Some(true) {
            builder = builder.set_mutable_content();
        }
        if let Some(ref category) = req.category {
            builder = builder.set_category(category);
        }

        let payload = builder.build(&req.device_token, Default::default());
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
}
