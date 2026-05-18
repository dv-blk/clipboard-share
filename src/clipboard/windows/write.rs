use std::borrow::Cow;

use arboard::{Clipboard, ImageData};

use crate::connection::payload::Payload;

/// Write a message to the clipboard.
pub async fn write(msg: Payload) -> anyhow::Result<()> {
    tokio::task::spawn_blocking(move || blocking_write(&msg)).await?
}

fn blocking_write(msg: &Payload) -> anyhow::Result<()> {
    let mut cb = Clipboard::new()?;
    match msg {
        Payload::Text(text) => {
            cb.set_text(text.clone())?;
        }
        Payload::Image {
            width,
            height,
            rgba,
        } => {
            let img = ImageData {
                width: *width as usize,
                height: *height as usize,
                bytes: Cow::Borrowed(rgba),
            };
            cb.set_image(img)?;
        }
        Payload::Heartbeat => {}
    }
    Ok(())
}
