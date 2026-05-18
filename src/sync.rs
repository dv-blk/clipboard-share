use std::time::Duration;

use tokio::sync::watch;
use tracing::{debug, error, info, warn};

use crate::clipboard::Clipboard;
use crate::connection::{ConnectionEvent, Peer};
use crate::echo_guard::EchoGuard;
use crate::payload::Payload;

#[derive(Default)]
pub struct Sync<C: Clipboard> {
    pub clipboard: C,
    pub echo_guard: EchoGuard,
}

impl<C: Clipboard> Sync<C> {
    pub async fn run(&self, mut conn: impl Peer) -> anyhow::Result<()> {
        let clipboard = self.clipboard.clone();
        let echo_guard = self.echo_guard.clone();
        let (content_tx, mut content_rx) = watch::channel(None);

        tokio::spawn(async move {
            info!("clipboard monitor started");
            let mut last_fp: Option<Vec<u8>> = None;

            loop {
                match clipboard.changed().await {
                    Ok(Some(msg)) => {
                        let fp = msg.fingerprint();
                        if echo_guard.is_echo(&fp) {
                            debug!("clipboard monitor: suppressing echo");
                        } else if last_fp.as_deref() == Some(fp.as_slice()) {
                            debug!("clipboard monitor: content unchanged, skipping");
                        } else {
                            info!("clipboard monitor: new content detected, publishing");
                            last_fp = Some(fp);
                            let _ = content_tx.send(Some(msg));
                        }
                    }
                    Ok(None) => {
                        debug!("clipboard monitor: selection event with no supported content");
                    }
                    Err(e) => {
                        error!("clipboard monitor error: {e}");
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                }
            }
        });

        content_rx.mark_unchanged();

        loop {
            tokio::select! {
                event = conn.recv() => {
                    match event {
                        Some(ConnectionEvent::Message(Payload::Heartbeat)) => {
                            debug!("sync engine: heartbeat received");
                        }
                        Some(ConnectionEvent::Message(msg)) => {
                            let fp = msg.fingerprint();
                            match self.clipboard.write(msg).await {
                                Err(e) => error!("sync engine: failed to write clipboard: {e}"),
                                Ok(_) => {
                                    self.echo_guard.record(fp);
                                    info!("sync engine: clipboard updated from peer");
                                }
                            }
                        }
                        Some(ConnectionEvent::Reconnected) => {
                            info!("sync engine: connected to peer");
                            if let Some(msg) = content_rx.borrow().clone() {
                                debug!("sync engine: flushing clipboard to new peer");
                                if let Err(e) = conn.send(msg).await {
                                    warn!("sync engine: flush send error: {e}");
                                }
                            }
                        }
                        Some(ConnectionEvent::Disconnected) => {
                            warn!("sync engine: disconnected from peer");
                        }
                        None => {
                            warn!("sync engine: connection loop closed, stopping");
                            break;
                        }
                    }
                }
                result = content_rx.changed() => {
                    let Ok(()) = result else {
                        warn!("sync engine: clipboard monitor channel closed, stopping");
                        break;
                    };
                    let Some(msg) = content_rx.borrow_and_update().clone() else { continue };
                    debug!("sync engine: clipboard changed, sending to peer");
                    match conn.send(msg).await {
                        Err(e) => warn!("sync engine: send error: {e}"),
                        Ok(_) => info!("sync engine: sent clipboard update to peer"),
                    }
                }
            }
        }

        Ok(())
    }
}
