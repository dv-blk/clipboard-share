use std::net::SocketAddr;

/// Runtime configuration parsed from CLI arguments.
#[derive(Debug, Clone)]
pub struct Config {
    /// Address to listen on (e.g. `0.0.0.0:9876`).
    pub listen: SocketAddr,
    /// Peer address to connect to (e.g. `192.168.1.42:9876`).
    pub peer: SocketAddr,
    /// How long (ms) to wait before retrying a failed outbound connection.
    pub reconnect_delay_ms: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen: "0.0.0.0:9876".parse().unwrap(),
            peer: "127.0.0.1:9876".parse().unwrap(),
            reconnect_delay_ms: 4000,
        }
    }
}
