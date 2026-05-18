use arboard::Clipboard;

use crate::payload::Payload;

/// Read the current clipboard contents.
pub async fn read() -> anyhow::Result<Option<Payload>> {
    tokio::task::spawn_blocking(|| {
        let mut cb = Clipboard::new()?;
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
    })
    .await?
}
