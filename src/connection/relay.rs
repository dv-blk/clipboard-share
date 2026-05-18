use tracing::{info, warn};

use crate::connection::{
    ConnectionEvent, ConnectionIo,
    payload::{read, write},
};

/// Drive a single active session. Returns `true` if the connection dropped and
/// we should reconnect, or `false` if the sync engine shut down the channel.
pub async fn relay<R, W>(reader: &mut R, writer: &mut W, io: &mut ConnectionIo) -> bool
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    loop {
        tokio::select! {
            result = read(reader) => {
                match result {
                    Ok(msg) => {
                        if !io.send_event(ConnectionEvent::Message(msg)).await {
                            return false;
                        }
                    }
                    Err(e) => {
                        if e.downcast_ref::<std::io::Error>()
                            .map(|e| matches!(e.kind(), std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::ConnectionReset))
                            .unwrap_or(false)
                        {
                            info!("connection: peer disconnected");
                        } else {
                            warn!("connection: receive error: {e}");
                        }
                        return true;
                    }
                }
            }
            msg = io.recv_msg() => {
                match msg {
                    Some(msg) => {
                        if let Err(e) = write(writer, &msg).await {
                            warn!("connection: send error: {e}");
                            return true;
                        }
                    }
                    None => return false,
                }
            }
        }
    }
}
