use std::io::Cursor;

use anyhow::Context;
use image::{ImageFormat, RgbaImage};
use wl_clipboard_rs::copy::{MimeType as CopyMimeType, Options, Source};

use crate::payload::Payload;

pub async fn write(msg: Payload) -> anyhow::Result<()> {
    tokio::task::spawn_blocking(move || {
        match msg {
            Payload::Text(text) => {
                let opts = Options::new();
                opts.copy(Source::Bytes(text.into_bytes().into()), CopyMimeType::Text)?;
            }
            Payload::Image {
                width,
                height,
                rgba,
            } => {
                let img = RgbaImage::from_raw(width, height, rgba)
                    .ok_or_else(|| anyhow::anyhow!("invalid image dimensions"))?;
                let mut png_bytes = Vec::new();
                img.write_to(&mut Cursor::new(&mut png_bytes), ImageFormat::Png)?;
                let opts = Options::new();
                opts.copy(
                    Source::Bytes(png_bytes.into()),
                    CopyMimeType::Specific("image/png".to_string()),
                )?;
            }
            Payload::Heartbeat => {}
        }
        Ok(())
    })
    .await
    .context("clipboard write task panicked")?
}
