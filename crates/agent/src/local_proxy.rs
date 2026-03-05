use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use url::Url;

pub fn compose_local_url(base: &str, path_and_query: &str) -> anyhow::Result<Url> {
    let mut normalized = base.to_string();
    if !normalized.ends_with('/') {
        normalized.push('/');
    }

    let base = Url::parse(&normalized)?;
    let without_leading_slash = path_and_query.trim_start_matches('/');
    Ok(base.join(without_leading_slash)?)
}

pub fn is_hop_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailers"
            | "transfer-encoding"
            | "upgrade"
    )
}

pub fn apply_forward_headers(
    mut req: reqwest::RequestBuilder,
    headers: &[(String, String)],
) -> reqwest::RequestBuilder {
    for (k, v) in headers {
        if is_hop_header(k) || k.eq_ignore_ascii_case("host") || k.eq_ignore_ascii_case("content-length") {
            continue;
        }
        let Ok(name) = HeaderName::from_bytes(k.as_bytes()) else {
            continue;
        };
        let Ok(value) = HeaderValue::from_str(v) else {
            continue;
        };
        req = req.header(name, value);
    }
    req
}

pub fn flatten_response_headers(headers: &HeaderMap) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (name, value) in headers {
        if is_hop_header(name.as_str()) {
            continue;
        }
        if let Ok(v) = value.to_str() {
            out.push((name.as_str().to_string(), v.to_string()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::compose_local_url;

    #[test]
    fn compose_local_url_keeps_query() {
        let url = compose_local_url("http://127.0.0.1:3000", "/api/test?q=1").expect("url");
        assert_eq!(url.as_str(), "http://127.0.0.1:3000/api/test?q=1");
    }
}
