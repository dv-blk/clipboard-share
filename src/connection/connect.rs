use std::{net::SocketAddr, time::Duration};

use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, info};

pub async fn connect(listen: SocketAddr, peer: SocketAddr) -> anyhow::Result<TcpStream> {
    let listener = TcpListener::bind(listen).await?;
    info!("listening on {listen}");

    tokio::select! {
        result = listener.accept() => {
            let (stream, addr) = result?;
            info!("accepted connection from {addr}");
            Ok(stream)
        }
        stream = async {
            loop {
                match TcpStream::connect(peer).await {
                    Ok(stream) => {
                        info!("connected outbound to {peer}");
                        return stream;
                    }
                    Err(e) => {
                        debug!("outbound connect to {peer} failed: {e}, retrying...");
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                }
            }
        } => Ok(stream),
    }
}
