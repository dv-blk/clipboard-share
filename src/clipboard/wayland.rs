use std::io::Read;

use anyhow::Context;
use image::{ImageFormat, RgbaImage};
use std::io::Cursor;
use tracing::debug;
use wl_clipboard_rs::copy::{MimeType as CopyMimeType, Options, Source};
use wl_clipboard_rs::paste::{
    get_contents, get_mime_types, ClipboardType, Error as PasteError, MimeType as PasteMimeType,
    Seat,
};

use crate::message::ClipboardMessage;

use super::ClipboardProvider;
use async_trait::async_trait;

/// Clipboard writer backed by `wl-clipboard-rs`. Used by the `SyncEngine`
/// receive task to write peer content to the local Wayland clipboard.
pub struct WaylandClipboard;

impl WaylandClipboard {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ClipboardProvider for WaylandClipboard {
    /// Reading is handled by [`read_next_change`] in the monitor.
    /// This impl reads once via get_contents for completeness / testing.
    async fn read(&mut self) -> anyhow::Result<Option<ClipboardMessage>> {
        tokio::task::spawn_blocking(blocking_read)
            .await
            .context("clipboard read task panicked")?
    }

    async fn write(&mut self, msg: &ClipboardMessage) -> anyhow::Result<()> {
        let msg = msg.clone();
        tokio::task::spawn_blocking(move || blocking_write(&msg))
            .await
            .context("clipboard write task panicked")?
    }
}

/// Read current clipboard contents synchronously. Called from a blocking thread.
pub fn blocking_read() -> anyhow::Result<Option<ClipboardMessage>> {
    // Try text first.
    match get_contents(ClipboardType::Regular, Seat::Unspecified, PasteMimeType::Text) {
        Ok((mut pipe, _mime)) => {
            let mut text = String::new();
            pipe.read_to_string(&mut text)?;
            if !text.is_empty() {
                debug!("wayland read: {} bytes of text", text.len());
                return Ok(Some(ClipboardMessage::Text(text)));
            }
        }
        Err(PasteError::ClipboardEmpty) | Err(PasteError::NoMimeType) => {}
        Err(e) => return Err(e.into()),
    }

    // Try image/png.
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
                return Ok(Some(ClipboardMessage::Image {
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
}

/// Block until the compositor fires a new `selection` event, then read and
/// return the new clipboard contents. Returns `None` if the new selection is
/// empty or contains no supported MIME type.
///
/// This is intended to be called repeatedly from a `tokio::task::spawn_blocking`
/// thread inside `ClipboardMonitor`.
pub fn blocking_wait_for_change() -> anyhow::Result<Option<ClipboardMessage>> {
    // get_mime_types blocks until the compositor sends the next selection event.
    let mime_types = match get_mime_types(ClipboardType::Regular, Seat::Unspecified) {
        Ok(types) => types,
        Err(PasteError::ClipboardEmpty) | Err(PasteError::NoMimeType) => return Ok(None),
        Err(e) => return Err(e.into()),
    };

    let has_text = mime_types.iter().any(|m| {
        m.starts_with("text/") || m == "TEXT" || m == "STRING" || m == "UTF8_STRING"
    });
    let has_image = mime_types.iter().any(|m| m == "image/png");

    if has_text {
        return blocking_read();
    }
    if has_image {
        return blocking_read();
    }

    Ok(None)
}

/// Write clipboard contents synchronously. Called from a blocking thread.
pub fn blocking_write(msg: &ClipboardMessage) -> anyhow::Result<()> {
    match msg {
        ClipboardMessage::Text(text) => {
            let opts = Options::new();
            opts.copy(
                Source::Bytes(text.as_bytes().to_vec().into()),
                CopyMimeType::Text,
            )?;
        }
        ClipboardMessage::Image { width, height, rgba } => {
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
        ClipboardMessage::Heartbeat => {}
    }
    Ok(())
}
