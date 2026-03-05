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
}

#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    pub listen: String,
    pub auth_tokens: HashSet<String>,
    pub request_timeout_secs: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct FileConfig {
    listen: Option<String>,
    auth_tokens: Option<Vec<String>>,
    request_timeout_secs: Option<u64>,
}

impl Config {
    pub fn resolve(self) -> anyhow::Result<ResolvedConfig> {
        let mut resolved = ResolvedConfig {
            listen: "0.0.0.0:8080".to_string(),
            auth_tokens: HashSet::new(),
            request_timeout_secs: 60,
        };

        if let Some(path) = self.config {
            let file_cfg = load_file_config(&path)?;
            if let Some(listen) = file_cfg.listen {
                resolved.listen = listen;
            }
            if let Some(tokens) = file_cfg.auth_tokens {
                resolved.auth_tokens =
                    normalize_token_list(tokens).map_err(anyhow::Error::msg)?;
            }
            if let Some(timeout) = file_cfg.request_timeout_secs {
                resolved.request_timeout_secs = timeout;
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
    Ok(())
}

fn parse_auth_tokens(raw: &str) -> Result<HashSet<String>, String> {
    if raw.trim().is_empty() {
        return Ok(HashSet::new());
    }
    normalize_token_list(raw.split(',').map(|token| token.to_string()).collect())
}

#[cfg(test)]
mod tests {
    use super::{parse_auth_tokens, validate_resolved, Config, ResolvedConfig};
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
        };

        let resolved = cfg.resolve().expect("resolve");
        assert_eq!(resolved.listen, "127.0.0.1:8080");
        assert_eq!(resolved.request_timeout_secs, 30);
        assert_eq!(resolved.auth_tokens, tokens);
    }

    #[test]
    fn validate_requires_tokens() {
        let cfg = ResolvedConfig {
            listen: "0.0.0.0:8080".to_string(),
            auth_tokens: HashSet::new(),
            request_timeout_secs: 60,
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
        };
        let err = cfg.resolve().expect_err("must fail");
        let _ = fs::remove_file(path);
        assert!(err.to_string().contains("auth_tokens"));
    }
}
