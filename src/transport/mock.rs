use crate::message::ClipboardMessage;
use async_trait::async_trait;
use tokio::sync::mpsc;
use super::{MessageSender, MessageReceiver};

/// Creates a pair of linked in-memory (sender, receiver) channels.
/// The sender on one side feeds the receiver on the other.
pub fn mock_transport_pair() -> (MockSender, MockReceiver) {
    let (tx, rx) = mpsc::channel(128);
    (MockSender { tx }, MockReceiver { rx })
}

pub struct MockSender {
    pub tx: mpsc::Sender<ClipboardMessage>,
}

pub struct MockReceiver {
    pub rx: mpsc::Receiver<ClipboardMessage>,
}

#[async_trait]
impl MessageSender for MockSender {
    async fn send(&mut self, msg: ClipboardMessage) -> anyhow::Result<()> {
        self.tx.send(msg).await.map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(())
    }
}

#[async_trait]
impl MessageReceiver for MockReceiver {
    async fn recv(&mut self) -> anyhow::Result<Option<ClipboardMessage>> {
        Ok(self.rx.recv().await)
    }
}
