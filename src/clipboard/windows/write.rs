use std::borrow::Cow;

use arboard::{Clipboard, ImageData};

use crate::payload::Payload;

pub async fn write(msg: Payload) -> anyhow::Result<()> {
    tokio::task::spawn_blocking(move || {
        let mut cb = Clipboard::new()?;
        match &msg {
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
    })
    .await?
}
