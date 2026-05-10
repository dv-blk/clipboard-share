use std::sync::Arc;

use tokio::sync::{watch, Mutex};
use tracing::{debug, error, info, warn};

use crate::clipboard::ClipboardProvider;
use crate::echo_guard::EchoGuard;
use crate::message::ClipboardMessage;
use crate::transport::{MessageReceiver, MessageSender};

/// Drives clipboard sync between the local machine and a remote peer for the
/// duration of a single TCP connection.
///
/// - **Send task**: watches the `ClipboardMonitor`'s `watch::Receiver` for
///   new local clipboard content and sends it to the peer over TCP.
/// - **Receive task**: reads messages from the peer over TCP, writes them to
///   the local clipboard via `ClipboardProvider`, and records the fingerprint
///   in `EchoGuard` so the monitor does not echo it back.
pub struct SyncEngine<W, S, R> {
    /// Write-only clipboard access — used by the receive task to apply peer content.
    clipboard_writer: Arc<Mutex<W>>,
    /// Subscribes to clipboard changes published by `ClipboardMonitor`.
    clipboard_rx: watch::Receiver<Option<ClipboardMessage>>,
    sender: S,
    receiver: R,
    echo_guard: EchoGuard,
}

impl<W, S, R> SyncEngine<W, S, R>
where
    W: ClipboardProvider + 'static,
    S: MessageSender + 'static,
    R: MessageReceiver + 'static,
{
    pub fn new(
        clipboard_writer: W,
        clipboard_rx: watch::Receiver<Option<ClipboardMessage>>,
        sender: S,
        receiver: R,
        echo_guard: EchoGuard,
    ) -> Self {
        Self {
            clipboard_writer: Arc::new(Mutex::new(clipboard_writer)),
            clipboard_rx,
            sender,
            receiver,
            echo_guard,
        }
    }

    /// Run until the connection drops or an unrecoverable error occurs.
    pub async fn run(self) -> anyhow::Result<()> {
        let SyncEngine {
            clipboard_writer,
            mut clipboard_rx,
            mut sender,
            mut receiver,
            echo_guard,
        } = self;

        // ── receive task ────────────────────────────────────────────────────
        let recv_handle = tokio::spawn(async move {
            loop {
                match receiver.recv().await {
                    Ok(Some(msg)) => {
                        if matches!(msg, ClipboardMessage::Heartbeat) {
                            debug!("heartbeat received");
                            continue;
                        }
                        let fp = msg.fingerprint();
                        // Record before writing so the monitor suppresses the echo.
                        echo_guard.record(fp);
                        let mut cb = clipboard_writer.lock().await;
                        if let Err(e) = cb.write(&msg).await {
                            error!("failed to write clipboard: {e}");
                        } else {
                            info!("clipboard updated from peer");
                        }
                    }
                    Ok(None) => {
                        info!("peer disconnected");
                        break;
                    }
                    Err(e) => {
                        error!("receive error: {e}");
                        break;
                    }
                }
            }
        });

        // ── send task (runs on current task) ─────────────────────────────────
        // Mark the current value as seen so we only send genuinely new changes.
        clipboard_rx.mark_unchanged();

        loop {
            if recv_handle.is_finished() {
                warn!("receive task finished, stopping sync engine");
                break;
            }

            match tokio::time::timeout(
                std::time::Duration::from_millis(100),
                clipboard_rx.changed(),
            )
            .await
            {
                Err(_) => {
                    // Timeout — loop back to check recv_handle.
                    continue;
                }
                Ok(Err(_)) => {
                    warn!("clipboard monitor channel closed, stopping sync engine");
                    break;
                }
                Ok(Ok(())) => {
                    let msg = clipboard_rx.borrow_and_update().clone();
                    let Some(msg) = msg else { continue };

                    debug!("clipboard changed, sending to peer");
                    match sender.send(msg).await {
                        Ok(()) => info!("sent clipboard update to peer"),
                        Err(e) => {
                            error!("send error: {e}");
                            break;
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard::mock::MockClipboard;
    use crate::transport::mock::{MockReceiver, MockSender};
    use std::time::Duration;
    use tokio::time;

    async fn run_engine_briefly<W, S, R>(engine: SyncEngine<W, S, R>, duration: Duration)
    where
        W: ClipboardProvider + 'static,
        S: MessageSender + 'static,
        R: MessageReceiver + 'static,
    {
        tokio::select! {
            _ = engine.run() => {}
            _ = time::sleep(duration) => {}
        }
    }

    #[tokio::test]
    async fn sends_clipboard_change_from_monitor() {
        let writer = MockClipboard::new();
        let echo_guard = EchoGuard::new();

        let (tx, rx) = watch::channel(None);
        let (_tx_inbound, inbound_rx) = tokio::sync::mpsc::channel::<ClipboardMessage>(16);
        let (tx_to_peer, mut rx_from_engine) = tokio::sync::mpsc::channel(16);

        let sender = MockSender { tx: tx_to_peer };
        let receiver = MockReceiver { rx: inbound_rx };

        let engine = SyncEngine::new(writer, rx, sender, receiver, echo_guard);

        // Send after engine starts so mark_unchanged() doesn't skip it.
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            tx.send(Some(ClipboardMessage::Text("hello".to_string()))).unwrap();
        });

        run_engine_briefly(engine, Duration::from_millis(200)).await;

        let msg = rx_from_engine.try_recv().expect("expected a message to be sent");
        assert_eq!(msg, ClipboardMessage::Text("hello".to_string()));
    }

    #[tokio::test]
    async fn writes_incoming_message_to_clipboard() {
        let writer = MockClipboard::new();
        let writer_check = writer.clone();
        let echo_guard = EchoGuard::new();

        let (_monitor_tx, rx) = watch::channel(None);
        let (tx_inbound, inbound_rx) = tokio::sync::mpsc::channel(16);
        let (tx_discard, _rx) = tokio::sync::mpsc::channel(16);

        let sender = MockSender { tx: tx_discard };
        let receiver = MockReceiver { rx: inbound_rx };

        tx_inbound
            .send(ClipboardMessage::Text("from peer".to_string()))
            .await
            .unwrap();
        drop(tx_inbound);

        let engine = SyncEngine::new(writer, rx, sender, receiver, echo_guard);
        run_engine_briefly(engine, Duration::from_millis(200)).await;

        assert_eq!(
            writer_check.get(),
            Some(ClipboardMessage::Text("from peer".to_string()))
        );
    }

    #[tokio::test]
    async fn echo_guard_recorded_after_receive() {
        let writer = MockClipboard::new();
        let echo_guard = EchoGuard::new();
        let echo_check = echo_guard.clone();

        let (_monitor_tx, rx) = watch::channel(None);
        let (tx_inbound, inbound_rx) = tokio::sync::mpsc::channel(16);
        let (tx_discard, _rx) = tokio::sync::mpsc::channel(16);

        let sender = MockSender { tx: tx_discard };
        let receiver = MockReceiver { rx: inbound_rx };

        let msg = ClipboardMessage::Text("peer content".to_string());
        let expected_fp = msg.fingerprint();

        tx_inbound.send(msg).await.unwrap();
        drop(tx_inbound);

        let engine = SyncEngine::new(writer, rx, sender, receiver, echo_guard);
        run_engine_briefly(engine, Duration::from_millis(200)).await;

        assert!(echo_check.is_echo(&expected_fp), "echo guard should record the received fingerprint");
    }
}
