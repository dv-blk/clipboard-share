mod read;
mod wait;
mod write;

use crate::clipboard::Clipboard;
use crate::connection::payload::Payload;

#[derive(Clone, Default)]
pub struct WindowsClipboard;

impl Clipboard for WindowsClipboard {
    fn wait(&self) -> impl std::future::Future<Output = anyhow::Result<Option<Payload>>> + Send {
        wait::clipboard_wait()
    }

    fn write(
        &self,
        payload: Payload,
    ) -> impl std::future::Future<Output = anyhow::Result<()>> + Send {
        write::write(payload)
    }
}
