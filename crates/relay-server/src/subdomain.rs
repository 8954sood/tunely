use std::net::IpAddr;

use anyhow::Context;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use tracing::{debug, warn};

use crate::config::DynamicSubdomainConfig;

const CLOUDFLARE_API_BASE: &str = "https://api.cloudflare.com/client/v4";
const MANAGED_COMMENT: &str = "managed-by=tunely";

#[derive(Debug, Clone)]
pub struct ProvisionedSubdomain {
    pub host: String,
    pub public_url: String,
}

#[derive(Debug, Clone)]
enum PublicOrigin {
    Ip(IpAddr),
    Host(String),
}

#[derive(Debug, Clone)]
pub struct SubdomainProvisioner {
    client: Client,
    cfg: DynamicSubdomainConfig,
    origin: PublicOrigin,
}

impl SubdomainProvisioner {
    pub fn new(cfg: DynamicSubdomainConfig) -> anyhow::Result<Self> {
        let origin = parse_public_origin(&cfg.public_origin)?;
        Ok(Self {
            client: Client::new(),
            cfg,
            origin,
        })
    }

    pub fn subdomain_host_for_tunnel(&self, tunnel_id: &str) -> String {
        format!("{tunnel_id}.{}", self.cfg.base_domain)
    }

    pub fn public_url_for_host(&self, host: &str) -> String {
        format!("https://{host}/")
    }

    pub async fn provision(&self, tunnel_id: &str) -> anyhow::Result<ProvisionedSubdomain> {
        let host = self.subdomain_host_for_tunnel(tunnel_id);
        debug!(%tunnel_id, %host, "starting subdomain provision");
        self.ensure_cloudflare_record(&host).await?;
        if let Err(err) = self.ensure_caddy_route(tunnel_id, &host).await {
            debug!(%tunnel_id, %host, error = %err, "provision failed; rolling back cloudflare record");
            let _ = self.delete_cloudflare_record(&host).await;
            return Err(err);
        }
        debug!(%tunnel_id, %host, "subdomain provision completed");
        Ok(ProvisionedSubdomain {
            public_url: self.public_url_for_host(&host),
            host,
        })
    }

    pub async fn deprovision(&self, tunnel_id: &str) -> anyhow::Result<()> {
        let host = self.subdomain_host_for_tunnel(tunnel_id);
        debug!(%tunnel_id, %host, "starting subdomain deprovision");
        if let Err(err) = self.delete_caddy_route(tunnel_id).await {
            warn!(error = %err, %tunnel_id, "failed to delete caddy route");
        }
        let result = self.delete_cloudflare_record(&host).await;
        if result.is_ok() {
            debug!(%tunnel_id, %host, "subdomain deprovision completed");
        }
        result
    }

    async fn ensure_cloudflare_record(&self, host: &str) -> anyhow::Result<()> {
        let desired = self.desired_record(host);
        debug!(
            host = %desired.name,
            record_type = %desired.record_type,
            content = %desired.content,
            "ensuring cloudflare record"
        );
        let existing = self.find_cloudflare_records(host).await?;
        debug!(host, existing_count = existing.len(), "cloudflare record lookup finished");
        if let Some(primary) = existing
            .iter()
            .find(|record| record.record_type == desired.record_type)
        {
            if !self.cfg.allow_existing_subdomain_resources {
                anyhow::bail!(
                    "cloudflare dns conflict: existing {} record for {} (record_id={}); set allow_existing_subdomain_resources=true to take over",
                    primary.record_type,
                    host,
                    primary.id
                );
            }
            let needs_update = primary.content != desired.content
                || primary.proxied.unwrap_or(false)
                || primary.comment.as_deref() != Some(MANAGED_COMMENT);
            if needs_update {
                debug!(host, record_id = %primary.id, "updating existing cloudflare record");
                self.put_cloudflare_record(&primary.id, &desired).await?;
            } else {
                debug!(host, record_id = %primary.id, "existing cloudflare record already matches desired state");
            }
        } else {
            debug!(host, "creating new cloudflare record");
            self.post_cloudflare_record(&desired).await?;
        }
        Ok(())
    }

    async fn delete_cloudflare_record(&self, host: &str) -> anyhow::Result<()> {
        let desired = self.desired_record(host);
        debug!(host, record_type = %desired.record_type, "deleting managed cloudflare records");
        let existing = self.find_cloudflare_records(host).await?;
        for record in existing {
            if record.record_type != desired.record_type {
                continue;
            }
            if record.comment.as_deref() != Some(MANAGED_COMMENT) {
                continue;
            }
            debug!(host, record_id = %record.id, "deleting cloudflare record");
            self.delete_cloudflare_record_by_id(&record.id).await?;
        }
        Ok(())
    }

    async fn ensure_caddy_route(&self, tunnel_id: &str, host: &str) -> anyhow::Result<()> {
        let route_id = route_id(tunnel_id);
        let route = json!({
            "@id": route_id,
            "match": [
                {
                    "host": [host],
                }
            ],
            "handle": [
                {
                    "handler": "rewrite",
                    "uri": format!("/t/{tunnel_id}{{uri}}"),
                },
                {
                    "handler": "reverse_proxy",
                    "upstreams": [
                        {
                            "dial": self.cfg.caddy_upstream,
                        }
                    ],
                }
            ],
            "terminal": true,
        });
        debug!(%tunnel_id, %host, %route_id, "ensuring caddy route");
        let exists = self.caddy_route_exists(&route_id).await?;
        debug!(%tunnel_id, %route_id, exists, "caddy route existence checked");
        if exists && !self.cfg.allow_existing_subdomain_resources {
            anyhow::bail!(
                "caddy route conflict: existing route_id={} for host {}; set allow_existing_subdomain_resources=true to take over",
                route_id,
                host
            );
        }

        let url = format!("{}/id/{}", self.caddy_admin_base(), route_id);
        if exists {
            debug!(%tunnel_id, %route_id, url = %url, "updating caddy route by id");
            let response = self.client.put(url).json(&route).send().await?;
            if response.status().is_success() {
                debug!(%tunnel_id, %route_id, "caddy route updated");
                return Ok(());
            }
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<failed to read response>".to_string());
            anyhow::bail!("caddy route upsert failed ({status}): {body}");
        }
        debug!(%tunnel_id, %route_id, "caddy route does not exist; creating route");
        self.create_caddy_route(&route).await
    }

    async fn create_caddy_route(&self, route: &serde_json::Value) -> anyhow::Result<()> {
        let servers_url = format!("{}/config/apps/http/servers", self.caddy_admin_base());
        debug!(url = %servers_url, "looking up caddy http servers");
        let servers_response = self.client.get(&servers_url).send().await?;
        if !servers_response.status().is_success() {
            let status = servers_response.status();
            let body = servers_response
                .text()
                .await
                .unwrap_or_else(|_| "<failed to read response>".to_string());
            anyhow::bail!("caddy servers lookup failed ({status}): {body}");
        }

        let servers: serde_json::Value = servers_response.json().await.context(
            "invalid caddy servers lookup response (expected JSON object at /config/apps/http/servers)",
        )?;
        let server_id = servers
            .as_object()
            .and_then(|o| {
                if o.contains_key("srv0") {
                    Some("srv0".to_string())
                } else {
                    o.keys().next().cloned()
                }
            })
            .context("no HTTP servers found in Caddy config")?;
        debug!(%server_id, "selected caddy server for route create");

        let routes_url = format!(
            "{}/config/apps/http/servers/{}/routes",
            self.caddy_admin_base(),
            server_id
        );
        debug!(url = %routes_url, "creating caddy route");
        let create_response = self.client.post(routes_url).json(route).send().await?;
        if !create_response.status().is_success() {
            let status = create_response.status();
            let body = create_response
                .text()
                .await
                .unwrap_or_else(|_| "<failed to read response>".to_string());
            anyhow::bail!("caddy route create failed ({status}): {body}");
        }
        debug!("caddy route created");
        Ok(())
    }

    async fn delete_caddy_route(&self, tunnel_id: &str) -> anyhow::Result<()> {
        let route_id = route_id(tunnel_id);
        let url = format!("{}/id/{}", self.caddy_admin_base(), route_id);
        debug!(%tunnel_id, %route_id, url = %url, "deleting caddy route");
        let response = self.client.delete(url).send().await?;
        if response.status().is_success() || response.status().as_u16() == 404 {
            debug!(%tunnel_id, %route_id, status = %response.status(), "caddy route delete completed");
            return Ok(());
        }
        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "<failed to read response>".to_string());
        anyhow::bail!("caddy route delete failed ({status}): {body}");
    }

    async fn caddy_route_exists(&self, route_id: &str) -> anyhow::Result<bool> {
        let url = format!("{}/id/{}", self.caddy_admin_base(), route_id);
        debug!(%route_id, url = %url, "checking caddy route existence");
        let response = self.client.get(url).send().await?;
        if response.status().is_success() {
            debug!(%route_id, "caddy route exists");
            return Ok(true);
        }
        if response.status().as_u16() == 404 {
            debug!(%route_id, "caddy route does not exist");
            return Ok(false);
        }
        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "<failed to read response>".to_string());
        anyhow::bail!("caddy route lookup failed ({status}): {body}");
    }

    fn desired_record(&self, host: &str) -> DesiredRecord {
        match &self.origin {
            PublicOrigin::Ip(ip) => match ip {
                IpAddr::V4(_) => DesiredRecord {
                    record_type: "A".to_string(),
                    name: host.to_string(),
                    content: ip.to_string(),
                },
                IpAddr::V6(_) => DesiredRecord {
                    record_type: "AAAA".to_string(),
                    name: host.to_string(),
                    content: ip.to_string(),
                },
            },
            PublicOrigin::Host(target) => DesiredRecord {
                record_type: "CNAME".to_string(),
                name: host.to_string(),
                content: target.clone(),
            },
        }
    }

    fn caddy_admin_base(&self) -> &str {
        self.cfg.caddy_admin_url.trim_end_matches('/')
    }

    fn cloudflare_base(&self) -> String {
        format!(
            "{CLOUDFLARE_API_BASE}/zones/{}/dns_records",
            self.cfg.cloudflare_zone_id
        )
    }

    async fn find_cloudflare_records(&self, host: &str) -> anyhow::Result<Vec<CloudflareRecord>> {
        debug!(%host, "querying cloudflare dns records");
        let response = self
            .client
            .get(self.cloudflare_base())
            .query(&[("name", host)])
            .bearer_auth(&self.cfg.cloudflare_api_token)
            .send()
            .await
            .context("cloudflare dns lookup failed")?;
        let status = response.status();
        let payload: CloudflareResponse<Vec<CloudflareRecord>> = response
            .json()
            .await
            .context("invalid cloudflare dns lookup response")?;
        if !status.is_success() || !payload.success {
            anyhow::bail!(
                "cloudflare dns lookup rejected: {}",
                format_cloudflare_errors(payload.errors)
            );
        }
        debug!(%host, count = payload.result.len(), "cloudflare dns lookup succeeded");
        Ok(payload.result)
    }

    async fn post_cloudflare_record(&self, record: &DesiredRecord) -> anyhow::Result<()> {
        let body = json!({
            "type": record.record_type,
            "name": record.name,
            "content": record.content,
            "proxied": false,
            "ttl": 1,
            "comment": MANAGED_COMMENT,
        });
        let response = self
            .client
            .post(self.cloudflare_base())
            .bearer_auth(&self.cfg.cloudflare_api_token)
            .json(&body)
            .send()
            .await
            .context("cloudflare dns create failed")?;
        self.check_cloudflare_result(response, "create").await
    }

    async fn put_cloudflare_record(
        &self,
        record_id: &str,
        record: &DesiredRecord,
    ) -> anyhow::Result<()> {
        let body = json!({
            "type": record.record_type,
            "name": record.name,
            "content": record.content,
            "proxied": false,
            "ttl": 1,
            "comment": MANAGED_COMMENT,
        });
        let response = self
            .client
            .put(format!("{}/{}", self.cloudflare_base(), record_id))
            .bearer_auth(&self.cfg.cloudflare_api_token)
            .json(&body)
            .send()
            .await
            .context("cloudflare dns update failed")?;
        self.check_cloudflare_result(response, "update").await
    }

    async fn delete_cloudflare_record_by_id(&self, record_id: &str) -> anyhow::Result<()> {
        let response = self
            .client
            .delete(format!("{}/{}", self.cloudflare_base(), record_id))
            .bearer_auth(&self.cfg.cloudflare_api_token)
            .send()
            .await
            .context("cloudflare dns delete failed")?;
        self.check_cloudflare_result(response, "delete").await
    }

    async fn check_cloudflare_result(
        &self,
        response: reqwest::Response,
        op: &str,
    ) -> anyhow::Result<()> {
        let status = response.status();
        let payload: CloudflareResponse<serde_json::Value> = response
            .json()
            .await
            .context("invalid cloudflare response payload")?;
        if !status.is_success() || !payload.success {
            anyhow::bail!(
                "cloudflare dns {op} rejected: {}",
                format_cloudflare_errors(payload.errors)
            );
        }
        Ok(())
    }
}

fn route_id(tunnel_id: &str) -> String {
    format!("tunely-subdomain-{tunnel_id}")
}

pub fn is_valid_dns_label(label: &str) -> bool {
    if label.is_empty() || label.len() > 63 {
        return false;
    }
    let bytes = label.as_bytes();
    if bytes.first().is_some_and(|b| *b == b'-') || bytes.last().is_some_and(|b| *b == b'-') {
        return false;
    }
    label
        .bytes()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}

fn parse_public_origin(value: &str) -> anyhow::Result<PublicOrigin> {
    let trimmed = value.trim();
    if let Ok(ip) = trimmed.parse::<IpAddr>() {
        return Ok(PublicOrigin::Ip(ip));
    }
    if trimmed.is_empty() || trimmed.contains("://") || trimmed.contains('/') {
        anyhow::bail!("public_origin must be an IP address or hostname");
    }
    Ok(PublicOrigin::Host(trimmed.to_string()))
}

fn format_cloudflare_errors(errors: Vec<CloudflareError>) -> String {
    if errors.is_empty() {
        return "unknown error".to_string();
    }
    errors
        .into_iter()
        .map(|error| format!("{}: {}", error.code, error.message))
        .collect::<Vec<_>>()
        .join(", ")
}

#[derive(Debug)]
struct DesiredRecord {
    record_type: String,
    name: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct CloudflareResponse<T> {
    success: bool,
    #[serde(default)]
    errors: Vec<CloudflareError>,
    result: T,
}

#[derive(Debug, Deserialize)]
struct CloudflareError {
    code: u64,
    message: String,
}

#[derive(Debug, Deserialize)]
struct CloudflareRecord {
    id: String,
    #[serde(rename = "type")]
    record_type: String,
    content: String,
    #[serde(default)]
    proxied: Option<bool>,
    #[serde(default)]
    comment: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::is_valid_dns_label;

    #[test]
    fn dns_label_validation() {
        assert!(is_valid_dns_label("demo-1"));
        assert!(!is_valid_dns_label("Demo"));
        assert!(!is_valid_dns_label("demo_1"));
        assert!(!is_valid_dns_label("-demo"));
        assert!(!is_valid_dns_label("demo-"));
    }
}
