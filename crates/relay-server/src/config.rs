use std::collections::HashMap;

use clap::Parser;

#[derive(Debug, Clone, Parser)]
#[command(name = "relay-server")]
pub struct Config {
    #[arg(long, default_value = "0.0.0.0:8080")]
    pub listen: String,
    #[arg(long, value_parser = parse_auth_map, help = "tunnel=token,tunnel2=token2")]
    pub auth: HashMap<String, String>,
    #[arg(long, default_value_t = 60)]
    pub request_timeout_secs: u64,
}

fn parse_auth_map(raw: &str) -> Result<HashMap<String, String>, String> {
    let mut out = HashMap::new();
    if raw.trim().is_empty() {
        return Ok(out);
    }

    for pair in raw.split(',') {
        let mut parts = pair.splitn(2, '=');
        let tunnel = parts
            .next()
            .ok_or_else(|| format!("invalid auth pair: {pair}"))?
            .trim();
        let token = parts
            .next()
            .ok_or_else(|| format!("invalid auth pair: {pair}"))?
            .trim();

        if tunnel.is_empty() || token.is_empty() {
            return Err(format!("invalid auth pair: {pair}"));
        }

        out.insert(tunnel.to_string(), token.to_string());
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::parse_auth_map;

    #[test]
    fn parse_ok() {
        let map = parse_auth_map("demo=xxx,foo=bar").expect("parse");
        assert_eq!(map.get("demo"), Some(&"xxx".to_string()));
        assert_eq!(map.get("foo"), Some(&"bar".to_string()));
    }

    #[test]
    fn parse_empty() {
        let map = parse_auth_map("").expect("parse");
        assert!(map.is_empty());
    }
}
