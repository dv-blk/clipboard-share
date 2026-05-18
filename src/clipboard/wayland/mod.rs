mod changed;
mod read;
mod write;

use crate::clipboard::Clipboard;
use crate::payload::Payload;

#[derive(Clone, Default)]
pub struct WaylandClipboard;

impl Clipboard for WaylandClipboard {
    fn changed(&self) -> impl std::future::Future<Output = anyhow::Result<Option<Payload>>> + Send {
        changed::changed()
    }

    fn write(
        &self,
        payload: Payload,
    ) -> impl std::future::Future<Output = anyhow::Result<()>> + Send {
        write::write(payload)
    }
}
