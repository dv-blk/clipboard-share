use std::io::{Cursor, Read};

use anyhow::Context;
use image::ImageFormat;
use tracing::debug;
use wl_clipboard_rs::paste::{
    ClipboardType, Error as PasteError, MimeType as PasteMimeType, Seat, get_contents,
};

use crate::payload::Payload;

pub async fn read() -> anyhow::Result<Option<Payload>> {
    tokio::task::spawn_blocking(|| {
        match get_contents(
            ClipboardType::Regular,
            Seat::Unspecified,
            PasteMimeType::Text,
        ) {
            Ok((mut pipe, _mime)) => {
                let mut text = String::new();
                pipe.read_to_string(&mut text)?;
                if !text.is_empty() {
                    debug!("wayland read: {} bytes of text", text.len());
                    return Ok(Some(Payload::Text(text)));
                }
            }
            Err(PasteError::ClipboardEmpty) | Err(PasteError::NoMimeType) => {}
            Err(e) => return Err(e.into()),
        }

        match get_contents(
            ClipboardType::Regular,
            Seat::Unspecified,
            PasteMimeType::Specific("image/png"),
        ) {
            Ok((mut pipe, _mime)) => {
                let mut buf = Vec::new();
                pipe.read_to_end(&mut buf)?;
                if !buf.is_empty() {
                    debug!("wayland read: {} bytes of PNG", buf.len());
                    let img = image::load(Cursor::new(&buf), ImageFormat::Png)?.into_rgba8();
                    let (width, height) = img.dimensions();
                    return Ok(Some(Payload::Image {
                        width,
                        height,
                        rgba: img.into_raw(),
                    }));
                }
            }
            Err(PasteError::ClipboardEmpty) | Err(PasteError::NoMimeType) => {}
            Err(e) => return Err(e.into()),
        }

        Ok(None)
    })
    .await
    .context("clipboard read task panicked")?
}
