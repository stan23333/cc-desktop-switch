use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

const COMMON_PROXY_PORTS: [u16; 12] = [
    7890,  // Clash / ClashX / Clash Verge
    7897,  // Clash Verge Rev
    7891,  // Clash SOCKS, some builds also expose HTTP
    6152,  // Surge HTTP
    6153,  // Surge SOCKS
    1080,  // Shadowsocks / SSR / v2rayN
    10808, // v2rayN SOCKS
    10809, // v2rayN HTTP
    1082,  // Shadowrocket
    8118,  // Privoxy
    8888,  // Fiddler / Charles
    8889,  // Surge Mac
];

pub fn detect_local_proxy() -> String {
    if let Some(value) = detect_from_env() {
        return value;
    }

    detect_from_ports(&COMMON_PROXY_PORTS, Duration::from_secs(1)).unwrap_or_default()
}

fn detect_from_env() -> Option<String> {
    let env_vars: Vec<(String, String)> = std::env::vars().collect();
    detect_from_env_pairs(&env_vars)
}

fn detect_from_env_pairs(vars: &[(String, String)]) -> Option<String> {
    for name in ["HTTPS_PROXY", "HTTP_PROXY", "ALL_PROXY"] {
        if let Some(value) = env_value(vars, name).and_then(normalize_proxy_value) {
            return Some(value);
        }
        if let Some(value) =
            env_value(vars, &name.to_ascii_lowercase()).and_then(normalize_proxy_value)
        {
            return Some(value);
        }
    }
    None
}

fn env_value<'a>(vars: &'a [(String, String)], key: &str) -> Option<&'a str> {
    vars.iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
}

fn normalize_proxy_value(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn detect_from_ports(ports: &[u16], timeout: Duration) -> Option<String> {
    ports.iter().find_map(|port| {
        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, *port));
        TcpStream::connect_timeout(&address, timeout).ok()?;
        Some(format!("http://127.0.0.1:{port}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_detection_prefers_https_then_http_then_all_proxy() {
        let vars = vec![
            ("ALL_PROXY".to_string(), "http://127.0.0.1:1080".to_string()),
            (
                "HTTP_PROXY".to_string(),
                "http://127.0.0.1:7890".to_string(),
            ),
            (
                "HTTPS_PROXY".to_string(),
                " http://127.0.0.1:7897 ".to_string(),
            ),
        ];

        assert_eq!(
            detect_from_env_pairs(&vars).as_deref(),
            Some("http://127.0.0.1:7897")
        );
    }

    #[test]
    fn env_detection_accepts_lowercase_names() {
        let vars = vec![(
            "https_proxy".to_string(),
            "socks5://127.0.0.1:6153".to_string(),
        )];

        assert_eq!(
            detect_from_env_pairs(&vars).as_deref(),
            Some("socks5://127.0.0.1:6153")
        );
    }

    #[test]
    fn env_detection_ignores_empty_values() {
        let vars = vec![
            ("HTTPS_PROXY".to_string(), " ".to_string()),
            (
                "HTTP_PROXY".to_string(),
                "http://127.0.0.1:7890".to_string(),
            ),
        ];

        assert_eq!(
            detect_from_env_pairs(&vars).as_deref(),
            Some("http://127.0.0.1:7890")
        );
    }
}
