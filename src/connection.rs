use std::{net::SocketAddr, sync::Arc, time::Duration};

use tokio::{
    io::AsyncWriteExt,
    net::{TcpListener, TcpStream},
    sync::mpsc,
};
use tracing::{debug, error, info, warn};

use crate::payload::Payload;

pub fn bind_listener(listen: SocketAddr) -> anyhow::Result<Arc<TcpListener>> {
    let socket = tokio::net::TcpSocket::new_v4()?;
    socket.set_reuseaddr(true)?;
    socket.bind(listen)?;
    let listener = socket.listen(128)?;
    info!("listening on {listen}");
    Ok(Arc::new(listener))
}

/// Events produced by the connection loop and consumed by `Sync`.
pub enum ConnectionEvent {
    /// A new connection to the peer has been established.
    Reconnected,
    /// The connection to the peer was lost.
    Disconnected,
    /// A message was received from the peer.
    Message(Payload),
}

/// Abstraction over a peer connection, allowing `Sync` to be tested
/// without a real TCP stack.
pub trait Peer {
    fn send(
        &self,
        payload: Payload,
    ) -> impl std::future::Future<Output = anyhow::Result<()>> + Send;
    fn recv(&mut self) -> impl std::future::Future<Output = Option<ConnectionEvent>> + Send;
}

/// A live peer connection. Drives a background reconnect loop and exposes
/// methods for sending messages and receiving events.
pub struct Connection {
    msg_tx: mpsc::Sender<Payload>,
    event_rx: mpsc::Receiver<ConnectionEvent>,
}

impl Connection {
    pub fn open(listener: Arc<TcpListener>, peer: SocketAddr, reconnect_delay: Duration) -> Self {
        let (msg_tx, mut msg_rx) = mpsc::channel::<Payload>(64);
        let (event_tx, event_rx) = mpsc::channel::<ConnectionEvent>(64);

        tokio::spawn(async move {
            loop {
                info!("connection: waiting for connection...");
                let Ok(stream) = connect(&listener, peer).await else {
                    error!("connection: connect failed");
                    tokio::time::sleep(reconnect_delay).await;
                    continue;
                };

                let (mut reader, mut writer) = tokio::io::split(stream);

                info!("connection: connected");
                let Ok(()) = event_tx.send(ConnectionEvent::Reconnected).await else {
                    return;
                };

                'relay: loop {
                    tokio::select! {
                        result = Payload::read_from(&mut reader) => {
                            match result {
                                Ok(msg) => {
                                    let Ok(()) = event_tx.send(ConnectionEvent::Message(msg)).await else { return; };
                                }
                                Err(e) => {
                                    if is_disconnect_error(&e) {
                                        info!("connection: peer disconnected");
                                    } else {
                                        warn!("connection: receive error: {e}");
                                    }
                                    break 'relay;
                                }
                            }
                        }
                        msg = msg_rx.recv() => {
                            let Some(msg) = msg else { return };
                            if let Err(e) = msg.write_to(&mut writer).await {
                                warn!("connection: send error: {e}");
                                break 'relay;
                            }
                        }
                    }
                }

                let _ = writer.shutdown().await;
                let Ok(()) = event_tx.send(ConnectionEvent::Disconnected).await else {
                    return;
                };

                info!("connection: reconnecting in {reconnect_delay:?}...");
                tokio::time::sleep(reconnect_delay).await;
            }
        });

        Self { msg_tx, event_rx }
    }
}

impl Peer for Connection {
    async fn send(&self, p: Payload) -> anyhow::Result<()> {
        self.msg_tx
            .send(p)
            .await
            .map_err(|_| anyhow::anyhow!("connection loop closed"))
    }

    async fn recv(&mut self) -> Option<ConnectionEvent> {
        self.event_rx.recv().await
    }
}

async fn connect(listener: &TcpListener, peer: SocketAddr) -> anyhow::Result<TcpStream> {
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

fn is_disconnect_error(e: &anyhow::Error) -> bool {
    e.downcast_ref::<std::io::Error>()
        .map(|e| {
            matches!(
                e.kind(),
                std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::ConnectionReset
            )
        })
        .unwrap_or(false)
}
