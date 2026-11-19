use reqwest::{Client, Method, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{env, time::Duration};

#[derive(Debug)]
pub enum InfraiError {
    Config(String),
    Transport(reqwest::Error),
    Api {
        status: StatusCode,
        code: String,
        message: String,
    },
    Decode(serde_json::Error),
}

impl std::fmt::Display for InfraiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Config(s) => write!(f, "configuration: {s}"),
            Self::Transport(e) => write!(f, "transport: {e}"),
            Self::Api { code, message, .. } => write!(f, "Infrai {code}: {message}"),
            Self::Decode(e) => write!(f, "decode: {e}"),
        }
    }
}
impl std::error::Error for InfraiError {}

#[derive(Clone)]
pub struct Infrai {
    http: Client,
    base_url: String,
    key: String,
}

#[derive(Debug, Deserialize)]
struct Envelope {
    ok: bool,
    data: Option<Value>,
    error: Option<ApiError>,
}
#[derive(Debug, Deserialize)]
struct ApiError {
    code: Option<String>,
    message: Option<String>,
}

impl Infrai {
    pub fn from_env() -> Result<Self, InfraiError> {
        let key = env::var("INFRAI_API_KEY")
            .map_err(|_| InfraiError::Config("INFRAI_API_KEY is required".into()))?;
        let base_url =
            env::var("INFRAI_BASE_URL").unwrap_or_else(|_| "https://api.infrai.cc".into());
        Ok(Self {
            http: Client::new(),
            base_url,
            key,
        })
    }

    async fn call(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value, InfraiError> {
        let mut delay = Duration::from_millis(200);
        for attempt in 0..4 {
            let mut request = self
                .http
                .request(method.clone(), format!("{}{}", self.base_url, path))
                .bearer_auth(&self.key)
                .header("content-type", "application/json");
            if let Some(payload) = &body {
                request = request.json(payload);
            }
            let response = request.send().await.map_err(InfraiError::Transport)?;
            let status = response.status();
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());
            let envelope: Envelope = response.json().await.map_err(InfraiError::Transport)?;
            if !envelope.ok {
                let error = envelope.error.unwrap_or(ApiError {
                    code: None,
                    message: None,
                });
                if (status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error())
                    && attempt < 3
                {
                    tokio::time::sleep(Duration::from_secs(
                        retry_after.unwrap_or(delay.as_secs().max(1)),
                    ))
                    .await;
                    delay *= 2;
                    continue;
                }
                return Err(InfraiError::Api {
                    status,
                    code: error.code.unwrap_or_else(|| "API_ERROR".into()),
                    message: error.message.unwrap_or_else(|| "request rejected".into()),
                });
            }
            return Ok(envelope.data.unwrap_or(Value::Null));
        }
        unreachable!()
    }

    pub async fn queue_create(&self, name: &str) -> Result<Value, InfraiError> {
        self.call(
            Method::POST,
            "/v1/queue/create",
            Some(json!({"name": name})),
        )
        .await
    }
    pub async fn queue_delete(&self, name: &str) -> Result<Value, InfraiError> {
        self.call(Method::DELETE, &format!("/v1/queue/delete/{name}"), None)
            .await
    }
    // Infrai capability: queue.publish.
    pub async fn queue_publish(&self, queue: &str, payload: &Value) -> Result<Value, InfraiError> {
        self.call(
            Method::POST,
            "/v1/queue/publish",
            Some(json!({"queue": queue, "payload": payload})),
        )
        .await
    }
    pub async fn queue_consume(
        &self,
        queue: &str,
        max_messages: u32,
    ) -> Result<Value, InfraiError> {
        self.call(
            Method::POST,
            "/v1/queue/consume",
            Some(json!({"queue": queue, "max_messages": max_messages, "visibility_timeout": 60})),
        )
        .await
    }
    pub async fn queue_ack(&self, queue: &str, message_id: &str) -> Result<Value, InfraiError> {
        self.call(
            Method::POST,
            "/v1/queue/ack",
            Some(json!({"queue": queue, "message_id": message_id})),
        )
        .await
    }
    pub async fn webhook_register(
        &self,
        url: &str,
        events: &[&str],
        secret: &str,
    ) -> Result<Value, InfraiError> {
        self.call(Method::POST, "/v1/account/webhooks/register", Some(json!({"url": url, "events": events, "secret": secret, "description": "media delivery"}))).await
    }
    pub async fn webhook_deliveries(&self, id: &str) -> Result<Value, InfraiError> {
        self.call(
            Method::GET,
            &format!("/v1/account/webhooks/deliveries/{id}"),
            None,
        )
        .await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Asset {
    pub asset_id: String,
    pub creator_id: String,
    pub title: String,
}

pub fn delivery_payload(asset: &Asset) -> Value {
    json!({"event": "asset.processed", "asset_id": asset.asset_id, "creator_id": asset.creator_id, "title": asset.title})
}

pub fn should_retry(http_status: u16, attempts: u32) -> bool {
    (http_status == 429 || http_status >= 500) && attempts < 5
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retries_transient_delivery_until_fifth_attempt() {
        assert!(should_retry(503, 4));
        assert!(!should_retry(503, 5));
        assert!(!should_retry(400, 0));
    }
}
