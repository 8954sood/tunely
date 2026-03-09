use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use serde::Deserialize;

#[derive(Debug, Clone, Parser)]
#[command(name = "relay-server")]
pub struct Config {
    #[arg(long)]
    pub config: Option<PathBuf>,
    #[arg(long)]
    pub listen: Option<String>,
    #[arg(
        long = "auth-token",
        value_parser = parse_auth_tokens,
        help = "token1,token2"
    )]
    pub auth_tokens: Option<HashSet<String>>,
    #[arg(long)]
    pub request_timeout_secs: Option<u64>,
    #[arg(long, num_args = 0..=1, default_missing_value = "true")]
    pub enable_dynamic_subdomain: Option<bool>,
    #[arg(long)]
    pub base_domain: Option<String>,
    #[arg(long)]
    pub cloudflare_api_token: Option<String>,
    #[arg(long)]
    pub cloudflare_zone_id: Option<String>,
    #[arg(long)]
    pub public_origin: Option<String>,
    #[arg(long)]
    pub caddy_admin_url: Option<String>,
    #[arg(long)]
    pub caddy_upstream: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DynamicSubdomainConfig {
    pub base_domain: String,
    pub cloudflare_api_token: String,
    pub cloudflare_zone_id: String,
    pub public_origin: String,
    pub caddy_admin_url: String,
    pub caddy_upstream: String,
}

#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    pub listen: String,
    pub auth_tokens: HashSet<String>,
    pub request_timeout_secs: u64,
    pub dynamic_subdomain: Option<DynamicSubdomainConfig>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct FileConfig {
    listen: Option<String>,
    auth_tokens: Option<Vec<String>>,
    request_timeout_secs: Option<u64>,
    enable_dynamic_subdomain: Option<bool>,
    base_domain: Option<String>,
    cloudflare_api_token: Option<String>,
    cloudflare_zone_id: Option<String>,
    public_origin: Option<String>,
    caddy_admin_url: Option<String>,
    caddy_upstream: Option<String>,
}

impl Config {
    pub fn resolve(self) -> anyhow::Result<ResolvedConfig> {
        let mut resolved = ResolvedConfig {
            listen: "0.0.0.0:8080".to_string(),
            auth_tokens: HashSet::new(),
            request_timeout_secs: 60,
            dynamic_subdomain: None,
        };
        let mut enable_dynamic_subdomain = false;
        let mut base_domain: Option<String> = None;
        let mut cloudflare_api_token: Option<String> = None;
        let mut cloudflare_zone_id: Option<String> = None;
        let mut public_origin: Option<String> = None;
        let mut caddy_admin_url: Option<String> = None;
        let mut caddy_upstream: Option<String> = None;

        if let Some(path) = self.config {
            let file_cfg = load_file_config(&path)?;
            if let Some(listen) = file_cfg.listen {
                resolved.listen = listen;
            }
            if let Some(tokens) = file_cfg.auth_tokens {
                resolved.auth_tokens = normalize_token_list(tokens).map_err(anyhow::Error::msg)?;
            }
            if let Some(timeout) = file_cfg.request_timeout_secs {
                resolved.request_timeout_secs = timeout;
            }
            if let Some(enabled) = file_cfg.enable_dynamic_subdomain {
                enable_dynamic_subdomain = enabled;
            }
            if let Some(value) = file_cfg.base_domain {
                base_domain = Some(value);
            }
            if let Some(value) = file_cfg.cloudflare_api_token {
                cloudflare_api_token = Some(value);
            }
            if let Some(value) = file_cfg.cloudflare_zone_id {
                cloudflare_zone_id = Some(value);
            }
            if let Some(value) = file_cfg.public_origin {
                public_origin = Some(value);
            }
            if let Some(value) = file_cfg.caddy_admin_url {
                caddy_admin_url = Some(value);
            }
            if let Some(value) = file_cfg.caddy_upstream {
                caddy_upstream = Some(value);
            }
        }

        if let Some(listen) = self.listen {
            resolved.listen = listen;
        }
        if let Some(tokens) = self.auth_tokens {
            resolved.auth_tokens = tokens;
        }
        if let Some(timeout) = self.request_timeout_secs {
            resolved.request_timeout_secs = timeout;
        }
        if let Some(enabled) = self.enable_dynamic_subdomain {
            enable_dynamic_subdomain = enabled;
        }
        if let Some(value) = self.base_domain {
            base_domain = Some(value);
        }
        if let Some(value) = self.cloudflare_api_token {
            cloudflare_api_token = Some(value);
        }
        if let Some(value) = self.cloudflare_zone_id {
            cloudflare_zone_id = Some(value);
        }
        if let Some(value) = self.public_origin {
            public_origin = Some(value);
        }
        if let Some(value) = self.caddy_admin_url {
            caddy_admin_url = Some(value);
        }
        if let Some(value) = self.caddy_upstream {
            caddy_upstream = Some(value);
        }

        if enable_dynamic_subdomain {
            let caddy_upstream = caddy_upstream.unwrap_or_else(|| resolved.listen.clone());
            resolved.dynamic_subdomain = Some(DynamicSubdomainConfig {
                base_domain: normalize_non_empty(base_domain, "base_domain")?,
                cloudflare_api_token: normalize_non_empty(
                    cloudflare_api_token,
                    "cloudflare_api_token",
                )?,
                cloudflare_zone_id: normalize_non_empty(cloudflare_zone_id, "cloudflare_zone_id")?,
                public_origin: normalize_non_empty(public_origin, "public_origin")?,
                caddy_admin_url: normalize_non_empty(caddy_admin_url, "caddy_admin_url")?,
                caddy_upstream: normalize_non_empty(Some(caddy_upstream), "caddy_upstream")?,
            });
        }

        validate_resolved(&resolved)?;
        Ok(resolved)
    }
}

fn load_file_config(path: &PathBuf) -> anyhow::Result<FileConfig> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read config file: {}", path.display()))?;
    if raw.trim().is_empty() {
        return Ok(FileConfig::default());
    }

    let value: serde_yaml::Value = serde_yaml::from_str(&raw)
        .with_context(|| format!("invalid yaml in config file: {}", path.display()))?;
    if value.is_null() {
        return Ok(FileConfig::default());
    }

    serde_yaml::from_value(value)
        .with_context(|| format!("invalid config schema in file: {}", path.display()))
}

fn normalize_token_list(tokens: Vec<String>) -> Result<HashSet<String>, String> {
    let mut out = HashSet::new();
    for token in tokens {
        let token = token.trim();
        if token.is_empty() {
            return Err("empty token is not allowed".to_string());
        }
        out.insert(token.to_string());
    }
    Ok(out)
}

fn validate_resolved(config: &ResolvedConfig) -> anyhow::Result<()> {
    if config.listen.trim().is_empty() {
        anyhow::bail!("listen must not be empty");
    }
    if config.request_timeout_secs == 0 {
        anyhow::bail!("request_timeout_secs must be >= 1");
    }
    if config.auth_tokens.is_empty() {
        anyhow::bail!("auth_tokens must not be empty (set in config file or --auth-token)");
    }
    if let Some(dynamic) = &config.dynamic_subdomain {
        if dynamic.base_domain.ends_with('.') || dynamic.base_domain.contains('/') {
            anyhow::bail!("base_domain must be a valid domain name");
        }
        if !dynamic.caddy_admin_url.starts_with("http://")
            && !dynamic.caddy_admin_url.starts_with("https://")
        {
            anyhow::bail!("caddy_admin_url must start with http:// or https://");
        }
    }
    Ok(())
}

fn parse_auth_tokens(raw: &str) -> Result<HashSet<String>, String> {
    if raw.trim().is_empty() {
        return Ok(HashSet::new());
    }
    normalize_token_list(raw.split(',').map(|token| token.to_string()).collect())
}

fn normalize_non_empty(value: Option<String>, field_name: &str) -> anyhow::Result<String> {
    let value = value.ok_or_else(|| {
        anyhow::anyhow!("{field_name} is required when dynamic subdomain is enabled")
    })?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        anyhow::bail!("{field_name} must not be empty");
    }
    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::{Config, ResolvedConfig, parse_auth_tokens, validate_resolved};
    use clap::Parser;
    use std::collections::HashSet;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn parse_ok() {
        let tokens = parse_auth_tokens("tok1,tok2").expect("parse");
        assert!(tokens.contains("tok1"));
        assert!(tokens.contains("tok2"));
    }

    #[test]
    fn parse_empty() {
        let tokens = parse_auth_tokens("").expect("parse");
        assert!(tokens.is_empty());
    }

    #[test]
    fn rejects_empty_item() {
        let err = parse_auth_tokens("tok1,").expect_err("must fail");
        assert!(err.contains("empty token"));
    }

    #[test]
    fn resolve_with_cli_only() {
        let mut tokens = HashSet::new();
        tokens.insert("tok".to_string());

        let cfg = Config {
            config: None,
            listen: Some("127.0.0.1:8080".to_string()),
            auth_tokens: Some(tokens.clone()),
            request_timeout_secs: Some(30),
            enable_dynamic_subdomain: None,
            base_domain: None,
            cloudflare_api_token: None,
            cloudflare_zone_id: None,
            public_origin: None,
            caddy_admin_url: None,
            caddy_upstream: None,
        };

        let resolved = cfg.resolve().expect("resolve");
        assert_eq!(resolved.listen, "127.0.0.1:8080");
        assert_eq!(resolved.request_timeout_secs, 30);
        assert_eq!(resolved.auth_tokens, tokens);
        assert!(resolved.dynamic_subdomain.is_none());
    }

    #[test]
    fn validate_requires_tokens() {
        let cfg = ResolvedConfig {
            listen: "0.0.0.0:8080".to_string(),
            auth_tokens: HashSet::new(),
            request_timeout_secs: 60,
            dynamic_subdomain: None,
        };
        let err = validate_resolved(&cfg).expect_err("must fail");
        assert!(err.to_string().contains("auth_tokens"));
    }

    #[test]
    fn empty_file_config_is_valid_input_but_needs_override() {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "tunely-relay-empty-config-{}-{}.yaml",
            std::process::id(),
            ts
        ));
        fs::write(&path, "").expect("write");
        let cfg = Config {
            config: Some(path.clone()),
            listen: None,
            auth_tokens: None,
            request_timeout_secs: None,
            enable_dynamic_subdomain: None,
            base_domain: None,
            cloudflare_api_token: None,
            cloudflare_zone_id: None,
            public_origin: None,
            caddy_admin_url: None,
            caddy_upstream: None,
        };
        let err = cfg.resolve().expect_err("must fail");
        let _ = fs::remove_file(path);
        assert!(err.to_string().contains("auth_tokens"));
    }

    #[test]
    fn dynamic_subdomain_requires_fields() {
        let mut tokens = HashSet::new();
        tokens.insert("tok".to_string());
        let cfg = Config {
            config: None,
            listen: Some("127.0.0.1:8080".to_string()),
            auth_tokens: Some(tokens),
            request_timeout_secs: Some(30),
            enable_dynamic_subdomain: Some(true),
            base_domain: Some("example.com".to_string()),
            cloudflare_api_token: None,
            cloudflare_zone_id: Some("zone".to_string()),
            public_origin: Some("1.2.3.4".to_string()),
            caddy_admin_url: Some("http://127.0.0.1:2019".to_string()),
            caddy_upstream: Some("127.0.0.1:8080".to_string()),
        };
        let err = cfg.resolve().expect_err("must fail");
        assert!(err.to_string().contains("cloudflare_api_token"));
    }

    #[test]
    fn dynamic_subdomain_flag_without_value_sets_true() {
        let cfg = Config::try_parse_from([
            "relay-server",
            "--enable-dynamic-subdomain",
            "--listen",
            "127.0.0.1:8080",
            "--auth-token",
            "tok",
        ])
        .expect("parse");
        assert_eq!(cfg.enable_dynamic_subdomain, Some(true));
    }
}
