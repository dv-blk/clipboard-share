use std::time::Duration;

use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, info, warn};

use crate::clipboard::ClipboardProvider;
use crate::clipboard_monitor::ClipboardMonitor;
use crate::config::Config;
use crate::echo_guard::EchoGuard;
use crate::sync_engine::SyncEngine;
use crate::transport::tcp::split_tcp;

/// Wires together all production dependencies and runs the sync loop.
///
/// The `ClipboardMonitor` starts immediately at process startup — clipboard
/// changes are captured before any peer connects and sent as soon as the
/// connection is established.
pub async fn run(config: Config) -> anyhow::Result<()> {
    let reconnect_delay = Duration::from_millis(config.reconnect_delay_ms);

    // Create the echo guard shared between monitor and engine.
    let echo_guard = EchoGuard::new();

    // Start the clipboard monitor — runs for the lifetime of the process.
    let (monitor, clipboard_rx) = ClipboardMonitor::new(echo_guard.clone());
    monitor.start();

    loop {
        info!("waiting for connection...");
        let stream = connect(&config).await?;
        let peer_addr = stream.peer_addr()?;
        info!("connected to {peer_addr}");

        let writer = make_writer()?;
        let (sender, receiver) = split_tcp(stream);
        let engine = SyncEngine::new(
            writer,
            clipboard_rx.clone(),
            sender,
            receiver,
            echo_guard.clone(),
        );

        if let Err(e) = engine.run().await {
            warn!("sync engine error: {e}");
        }

        info!("connection lost, reconnecting in {reconnect_delay:?}...");
        tokio::time::sleep(reconnect_delay).await;
    }
}

/// Create a write-only clipboard provider for the current platform.
fn make_writer() -> anyhow::Result<Box<dyn ClipboardProvider>> {
    #[cfg(target_os = "linux")]
    {
        use crate::clipboard::wayland::WaylandClipboard;
        info!("using Wayland clipboard writer");
        return Ok(Box::new(WaylandClipboard::new()));
    }

    #[cfg(target_os = "windows")]
    {
        use crate::clipboard::windows::WindowsClipboard;
        info!("using Windows clipboard writer");
        return Ok(Box::new(WindowsClipboard::new()?));
    }

    #[allow(unreachable_code)]
    Err(anyhow::anyhow!("unsupported platform"))
}

/// Race an inbound accept against an outbound connect attempt.
async fn connect(config: &Config) -> anyhow::Result<TcpStream> {
    let listener = TcpListener::bind(config.listen).await?;
    info!("listening on {}", config.listen);

    let peer = config.peer;

    tokio::select! {
        result = listener.accept() => {
            let (stream, addr) = result?;
            info!("accepted connection from {addr}");
            Ok(stream)
        }
        result = try_connect(peer) => {
            Ok(result?)
        }
    }
}

/// Repeatedly attempt to connect to `peer` until it succeeds.
async fn try_connect(peer: std::net::SocketAddr) -> anyhow::Result<TcpStream> {
    loop {
        match TcpStream::connect(peer).await {
            Ok(stream) => {
                info!("connected outbound to {peer}");
                return Ok(stream);
            }
            Err(e) => {
                debug!("outbound connect to {peer} failed: {e}, retrying...");
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }
}
