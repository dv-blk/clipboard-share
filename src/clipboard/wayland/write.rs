use std::io::Cursor;

use anyhow::Context;
use image::{ImageFormat, RgbaImage};
use wl_clipboard_rs::copy::{MimeType as CopyMimeType, Options, Source};

use crate::connection::payload::Payload;

/// Write a clipboard message.
pub async fn write(msg: Payload) -> anyhow::Result<()> {
    tokio::task::spawn_blocking(move || blocking_write(&msg))
        .await
        .context("clipboard write task panicked")?
}

fn blocking_write(msg: &Payload) -> anyhow::Result<()> {
    match msg {
        Payload::Text(text) => {
            let opts = Options::new();
            opts.copy(
                Source::Bytes(text.as_bytes().to_vec().into()),
                CopyMimeType::Text,
            )?;
        }
        Payload::Image {
            width,
            height,
            rgba,
        } => {
            let img = RgbaImage::from_raw(*width, *height, rgba.clone())
                .ok_or_else(|| anyhow::anyhow!("invalid image dimensions"))?;
            let mut png_bytes: Vec<u8> = Vec::new();
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
}
