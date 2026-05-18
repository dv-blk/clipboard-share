use std::{net::SocketAddr, time::Duration};

use tokio::sync::mpsc;

mod connect;
mod lifecycle;
pub mod payload;
mod relay;

use connect::connect;

/// The background task's side of the connection channels.
pub(super) struct ConnectionIo {
    pub(super) msg_rx: mpsc::Receiver<payload::Payload>,
    pub(super) event_tx: mpsc::Sender<ConnectionEvent>,
}

impl ConnectionIo {
    pub(super) async fn send_event(&self, event: ConnectionEvent) -> bool {
        self.event_tx.send(event).await.is_ok()
    }

    pub(super) async fn recv_msg(&mut self) -> Option<payload::Payload> {
        self.msg_rx.recv().await
    }
}

/// Events produced by the connection loop and consumed by `sync_engine`.
pub enum ConnectionEvent {
    /// A new connection to the peer has been established.
    Reconnected,
    /// The connection to the peer was lost.
    Disconnected,
    /// A message was received from the peer.
    Message(payload::Payload),
}

/// Abstraction over a peer connection, allowing sync_engine to be tested
/// without a real TCP stack.
pub trait Peer {
    async fn send(&self, payload: payload::Payload) -> anyhow::Result<()>;
    async fn next_event(&mut self) -> Option<ConnectionEvent>;
}

/// A live peer connection. Drives a background reconnect loop and exposes
/// methods for sending messages and receiving events.
pub struct Connection {
    msg_tx: mpsc::Sender<payload::Payload>,
    event_rx: mpsc::Receiver<ConnectionEvent>,
}

impl Connection {
    /// Open a connection using the real TCP connector.
    pub fn open(listen: SocketAddr, peer: SocketAddr, reconnect_delay: Duration) -> Self {
        let (msg_tx, msg_rx) = mpsc::channel::<payload::Payload>(64);
        let (event_tx, event_rx) = mpsc::channel::<ConnectionEvent>(64);

        tokio::spawn(lifecycle::run(
            move || connect(listen, peer),
            reconnect_delay,
            ConnectionIo { msg_rx, event_tx },
        ));

        Self { msg_tx, event_rx }
    }
}

impl Peer for Connection {
    async fn send(&self, p: payload::Payload) -> anyhow::Result<()> {
        self.msg_tx
            .send(p)
            .await
            .map_err(|_| anyhow::anyhow!("connection loop closed"))
    }

    async fn next_event(&mut self) -> Option<ConnectionEvent> {
        self.event_rx.recv().await
    }
}
