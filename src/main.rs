#![warn(rust_2018_idioms)]
#![deny(unused_must_use)]

mod clipboard;
mod connection;
mod echo_guard;
mod payload;
mod sync;

use std::{net::SocketAddr, time::Duration};

use clap::Parser;
use tracing::info;
use tracing_subscriber::EnvFilter;

use clipboard::PlatformClipboard;
use connection::Connection;
use sync::Sync;

#[derive(Parser, Debug)]
#[command(
    name = "clipboard-share",
    about = "Bidirectional clipboard sync over TCP"
)]
struct Cli {
    /// Local address to listen on.
    #[arg(long, default_value = "0.0.0.0:9876")]
    listen: SocketAddr,

    /// Peer address to connect to.
    #[arg(long)]
    peer: SocketAddr,

    /// Delay between reconnect attempts in milliseconds.
    #[arg(long, default_value_t = 4000)]
    reconnect_delay_ms: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("clipboard_share=info")),
        )
        .init();

    run(Cli::parse()).await
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    let conn = Connection::open(
        cli.listen,
        cli.peer,
        Duration::from_millis(cli.reconnect_delay_ms),
    );

    info!("initialized");

    Sync::<PlatformClipboard>::default().run(conn).await
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use tokio::sync::{Notify, mpsc};

    use crate::clipboard::Clipboard;
    use crate::connection::{ConnectionEvent, Peer};
    use crate::echo_guard::EchoGuard;
    use crate::payload::Payload;
    use crate::sync::Sync;
    #[derive(Default)]
    struct MockClipboardInner {
        contents: Mutex<Option<Payload>>,
        notify: Notify,
    }

    #[derive(Clone, Default)]
    struct MockClipboard {
        inner: Arc<MockClipboardInner>,
    }

    impl MockClipboard {
        fn new() -> Self {
            Self::default()
        }

        fn set(&self, msg: Payload) {
            *self.inner.contents.lock().unwrap() = Some(msg);
            self.inner.notify.notify_waiters();
        }

        fn get(&self) -> Option<Payload> {
            self.inner.contents.lock().unwrap().clone()
        }
    }

    impl Clipboard for MockClipboard {
        async fn changed(&self) -> anyhow::Result<Option<Payload>> {
            self.inner.notify.notified().await;
            Ok(self.inner.contents.lock().unwrap().clone())
        }

        async fn write(&self, msg: Payload) -> anyhow::Result<()> {
            *self.inner.contents.lock().unwrap() = Some(msg);
            self.inner.notify.notify_waiters();
            Ok(())
        }
    }

    struct MockConnection {
        event_rx: mpsc::Receiver<ConnectionEvent>,
        msg_tx: mpsc::Sender<Payload>,
    }

    struct MockConnectionHandle {
        event_tx: mpsc::Sender<ConnectionEvent>,
        msg_rx: mpsc::Receiver<Payload>,
    }

    impl MockConnection {
        fn new() -> (Self, MockConnectionHandle) {
            let (event_tx, event_rx) = mpsc::channel(64);
            let (msg_tx, msg_rx) = mpsc::channel(64);
            (
                Self { event_rx, msg_tx },
                MockConnectionHandle { event_tx, msg_rx },
            )
        }
    }

    impl Peer for MockConnection {
        async fn send(&self, payload: Payload) -> anyhow::Result<()> {
            self.msg_tx
                .send(payload)
                .await
                .map_err(|_| anyhow::anyhow!("mock connection closed"))
        }

        async fn recv(&mut self) -> Option<ConnectionEvent> {
            self.event_rx.recv().await
        }
    }

    #[tokio::test]
    async fn sends_clipboard_change_to_peer() {
        let (conn, mut handle) = MockConnection::new();
        let clipboard = MockClipboard::new();
        let clipboard_set = clipboard.clone();

        handle
            .event_tx
            .send(ConnectionEvent::Reconnected)
            .await
            .unwrap();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(80)).await;
            clipboard_set.set(Payload::Text("hello".to_string()));
        });

        let engine = Sync {
            clipboard,
            echo_guard: EchoGuard::default(),
        };
        tokio::select! {
            _ = engine.run(conn) => {}
            _ = tokio::time::sleep(Duration::from_millis(300)) => {}
        }

        let msg = handle.msg_rx.try_recv().expect("expected a message");
        assert_eq!(msg, Payload::Text("hello".to_string()));
    }

    #[tokio::test]
    async fn writes_incoming_message_to_clipboard() {
        let (conn, handle) = MockConnection::new();
        let writer = MockClipboard::new();
        let writer_check = writer.clone();

        handle
            .event_tx
            .send(ConnectionEvent::Message(Payload::Text(
                "from peer".to_string(),
            )))
            .await
            .unwrap();

        let engine = Sync {
            clipboard: writer,
            ..Sync::default()
        };
        tokio::select! {
            _ = engine.run(conn) => {}
            _ = tokio::time::sleep(Duration::from_millis(300)) => {}
        }

        assert_eq!(
            writer_check.get(),
            Some(Payload::Text("from peer".to_string()))
        );
    }

    #[tokio::test]
    async fn echo_guard_recorded_after_receive() {
        let (conn, handle) = MockConnection::new();
        let echo_guard = EchoGuard::default();
        let echo_check = echo_guard.clone();

        let writer = MockClipboard::new();
        let msg = Payload::Text("peer content".to_string());
        let expected_fp = msg.fingerprint();
        handle
            .event_tx
            .send(ConnectionEvent::Message(msg))
            .await
            .unwrap();

        let engine = Sync {
            clipboard: writer,
            echo_guard,
        };
        tokio::select! {
            _ = engine.run(conn) => {}
            _ = tokio::time::sleep(Duration::from_millis(300)) => {}
        }

        assert!(
            echo_check.is_echo(&expected_fp),
            "echo guard should record the received fingerprint"
        );
    }

    #[tokio::test]
    async fn echo_suppressed_after_peer_write() {
        let (conn, mut handle) = MockConnection::new();
        let clipboard = MockClipboard::new();
        let msg = Payload::Text("from peer".to_string());

        handle
            .event_tx
            .send(ConnectionEvent::Message(msg))
            .await
            .unwrap();

        let engine = Sync {
            clipboard,
            echo_guard: EchoGuard::default(),
        };
        tokio::select! {
            _ = engine.run(conn) => {}
            _ = tokio::time::sleep(Duration::from_millis(300)) => {}
        }

        assert!(
            handle.msg_rx.try_recv().is_err(),
            "echo should be suppressed: peer content must not be sent back"
        );
    }
}
