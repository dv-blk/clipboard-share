pub mod mock;
pub mod tcp;

use crate::message::ClipboardMessage;
use async_trait::async_trait;

/// Send clipboard messages to a peer.
#[async_trait]
pub trait MessageSender: Send {
    async fn send(&mut self, msg: ClipboardMessage) -> anyhow::Result<()>;
}

/// Receive clipboard messages from a peer. Returns `None` on clean disconnect.
#[async_trait]
pub trait MessageReceiver: Send {
    async fn recv(&mut self) -> anyhow::Result<Option<ClipboardMessage>>;
}
