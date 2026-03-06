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

#[cfg(test)]
mod tests {
    use super::is_hop_header;

    #[test]
    fn detects_hop_headers_case_insensitively() {
        assert!(is_hop_header("Connection"));
        assert!(is_hop_header("upgrade"));
        assert!(!is_hop_header("content-type"));
    }
}
