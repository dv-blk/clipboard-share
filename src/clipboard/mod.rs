pub mod mock;

#[cfg(target_os = "linux")]
pub mod wayland;

#[cfg(target_os = "windows")]
pub mod windows;

use crate::message::ClipboardMessage;
use async_trait::async_trait;

/// Abstraction over clipboard write (and optional read). Used by `SyncEngine`
/// to write incoming peer content to the local clipboard.
#[async_trait]
pub trait ClipboardProvider: Send + Sync {
    async fn read(&mut self) -> anyhow::Result<Option<ClipboardMessage>>;
    async fn write(&mut self, msg: &ClipboardMessage) -> anyhow::Result<()>;
}

/// Allow a boxed trait object to itself satisfy `ClipboardProvider`.
#[async_trait]
impl ClipboardProvider for Box<dyn ClipboardProvider> {
    async fn read(&mut self) -> anyhow::Result<Option<ClipboardMessage>> {
        (**self).read().await
    }

    async fn write(&mut self, msg: &ClipboardMessage) -> anyhow::Result<()> {
        (**self).write(msg).await
    }
}
