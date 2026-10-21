use std::net::{Ipv4Addr, TcpListener};

#[derive(Debug)]
pub struct StartupError;
impl std::fmt::Display for StartupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Local server startup failed")
    }
}
impl std::error::Error for StartupError {}

// Retain the bound socket until it is handed to Axum: no free-port lookup race.
pub fn bind(preferred: u16) -> std::io::Result<TcpListener> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, preferred))
        .or_else(|_| TcpListener::bind((Ipv4Addr::LOCALHOST, 0)))?;
    listener.set_nonblocking(true)?;
    Ok(listener)
}

#[cfg(test)]
mod tests {
    #[test]
    fn occupied_port_uses_another_loopback_port_and_keeps_ownership() {
        let occupied = super::bind(0).unwrap();
        let address = occupied.local_addr().unwrap();
        let fallback = super::bind(address.port()).unwrap();
        assert_ne!(fallback.local_addr().unwrap().port(), address.port());
        assert!(fallback.local_addr().unwrap().ip().is_loopback());
        assert!(std::net::TcpListener::bind(fallback.local_addr().unwrap()).is_err());
    }
}
