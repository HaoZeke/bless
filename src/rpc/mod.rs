pub mod client;
pub mod server;

/// `:port` is loopback, not all-interfaces.
pub fn resolve_tcp_addr(addr: &str) -> String {
    if addr.starts_with(':') {
        format!("127.0.0.1{addr}")
    } else {
        addr.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::resolve_tcp_addr;

    #[test]
    fn colon_port_is_localhost() {
        assert_eq!(resolve_tcp_addr(":9000"), "127.0.0.1:9000");
        assert_eq!(resolve_tcp_addr(":5678"), "127.0.0.1:5678");
    }

    #[test]
    fn explicit_addr_is_unchanged() {
        assert_eq!(resolve_tcp_addr("192.168.1.10:9000"), "192.168.1.10:9000");
        assert_eq!(resolve_tcp_addr("0.0.0.0:9000"), "0.0.0.0:9000");
        assert_eq!(resolve_tcp_addr("127.0.0.1:9000"), "127.0.0.1:9000");
    }
}
