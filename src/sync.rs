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
    pub async fn run<P: Peer + Send + 'static>(&self, peers: Vec<P>) -> anyhow::Result<()> {
        let (content_tx, content_rx) = watch::channel(None);
        self.watch_clipboard(content_tx);

        let mut set = tokio::task::JoinSet::new();

        for (i, peer) in peers.into_iter().enumerate() {
            self.sync_peer(&mut set, i, peer, content_rx.clone());
        }

        set.join_all().await;

        Ok(())
    }

    fn watch_clipboard(&self, content_tx: watch::Sender<Option<Payload>>) {
        let clipboard = self.clipboard.clone();
        let echo_guard = self.echo_guard.clone();
        tokio::spawn(async move {
            info!("clipboard monitor started");
            let mut last_fp: Option<Vec<u8>> = None;
            loop {
                match clipboard.changed().await {
                    Ok(Some(msg)) => {
                        let fp = msg.fingerprint();
                        if echo_guard.is_echo(&fp) {
                            debug!("clipboard monitor: suppressing echo");
                            last_fp = Some(fp);
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
    }

    fn sync_peer<P: Peer + Send + 'static>(
        &self,
        set: &mut tokio::task::JoinSet<()>,
        i: usize,
        peer: P,
        mut content_rx: watch::Receiver<Option<Payload>>,
    ) {
        let clipboard = self.clipboard.clone();
        let echo_guard = self.echo_guard.clone();
        content_rx.mark_unchanged();
        set.spawn(async move {
            let mut peer = peer;
            loop {
                tokio::select! {
                    event = peer.recv() => {
                        match event {
                            Some(ConnectionEvent::Message(Payload::Heartbeat)) => {
                                debug!("peer {i}: heartbeat received");
                            }
                            Some(ConnectionEvent::Message(msg)) => {
                                let fp = msg.fingerprint();
                                match clipboard.write(msg).await {
                                    Err(e) => error!("peer {i}: failed to write clipboard: {e}"),
                                    Ok(()) => {
                                        echo_guard.record(fp);
                                        info!("peer {i}: clipboard updated from peer");
                                    }
                                }
                            }
                            Some(ConnectionEvent::Reconnected) => {
                                info!("peer {i}: connected");
                                let msg = content_rx.borrow().clone();
                                if let Some(msg) = msg {
                                    debug!("peer {i}: flushing clipboard on reconnect");
                                    if let Err(e) = peer.send(msg).await {
                                        warn!("peer {i}: flush error: {e}");
                                    }
                                }
                            }
                            Some(ConnectionEvent::Disconnected) => {
                                warn!("peer {i}: disconnected");
                            }
                            None => {
                                warn!("peer {i}: connection loop closed");
                                return;
                            }
                        }
                    }
                    result = content_rx.changed() => {
                        let Ok(()) = result else { return };
                        let Some(msg) = content_rx.borrow_and_update().clone() else { continue };
                        debug!("peer {i}: clipboard changed, sending");
                        if let Err(e) = peer.send(msg).await {
                            warn!("peer {i}: send error: {e}");
                        }
                    }
                }
            }
        });
    }
}
