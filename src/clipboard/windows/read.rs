use std::sync::Mutex;

use arboard::Clipboard;

use crate::payload::Payload;

struct ArboardClipboard(Mutex<Clipboard>);

impl ArboardClipboard {
    fn new() -> anyhow::Result<Self> {
        Ok(Self(Mutex::new(Clipboard::new()?)))
    }

    fn read(&self) -> anyhow::Result<Option<Payload>> {
        let mut cb = self.0.lock().unwrap();
        match cb.get_text() {
            Ok(text) if !text.is_empty() => return Ok(Some(Payload::Text(text))),
            _ => {}
        }
        match cb.get_image() {
            Ok(img) => {
                return Ok(Some(Payload::Image {
                    width: img.width as u32,
                    height: img.height as u32,
                    rgba: img.bytes.into_owned(),
                }));
            }
            _ => {}
        }
        Ok(None)
    }
}

static CLIPBOARD: std::sync::OnceLock<ArboardClipboard> = std::sync::OnceLock::new();

fn clipboard() -> anyhow::Result<&'static ArboardClipboard> {
    CLIPBOARD.get_or_try_init(ArboardClipboard::new)
}

/// Read the current clipboard contents.
pub async fn read() -> anyhow::Result<Option<Payload>> {
    let cb = clipboard()?;
    tokio::task::spawn_blocking(move || cb.read()).await?
}
