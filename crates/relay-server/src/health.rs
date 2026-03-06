use axum::Json;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct HealthPayload {
    pub status: &'static str,
    pub service: &'static str,
    pub version: &'static str,
}

pub async fn healthz() -> Json<HealthPayload> {
    Json(HealthPayload {
        status: "ok",
        service: "relay-server",
        version: env!("CARGO_PKG_VERSION"),
    })
}

pub async fn readyz() -> Json<HealthPayload> {
    Json(HealthPayload {
        status: "ready",
        service: "relay-server",
        version: env!("CARGO_PKG_VERSION"),
    })
}

#[cfg(test)]
mod tests {
    use super::{healthz, readyz};
    use axum::Json;

    #[tokio::test]
    async fn healthz_returns_ok_payload() {
        let Json(payload) = healthz().await;
        assert_eq!(payload.status, "ok");
        assert_eq!(payload.service, "relay-server");
        assert!(!payload.version.is_empty());
    }

    #[tokio::test]
    async fn readyz_returns_ready_payload() {
        let Json(payload) = readyz().await;
        assert_eq!(payload.status, "ready");
        assert_eq!(payload.service, "relay-server");
        assert!(!payload.version.is_empty());
    }
}
