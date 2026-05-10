use crate::message::ClipboardMessage;
use async_trait::async_trait;
use arboard::{Clipboard, ImageData};
use std::borrow::Cow;
use super::ClipboardProvider;

/// Clipboard backed by `arboard`. Windows only.
pub struct WindowsClipboard {
    inner: Clipboard,
}

impl WindowsClipboard {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            inner: Clipboard::new()?,
        })
    }

    pub fn read_blocking(&mut self) -> anyhow::Result<Option<ClipboardMessage>> {
        match self.inner.get_text() {
            Ok(text) if !text.is_empty() => {
                return Ok(Some(ClipboardMessage::Text(text)));
            }
            _ => {}
        }
        match self.inner.get_image() {
            Ok(img) => {
                return Ok(Some(ClipboardMessage::Image {
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

#[async_trait]
impl ClipboardProvider for WindowsClipboard {
    async fn read(&mut self) -> anyhow::Result<Option<ClipboardMessage>> {
        match self.inner.get_text() {
            Ok(text) if !text.is_empty() => {
                return Ok(Some(ClipboardMessage::Text(text)));
            }
            _ => {}
        }
        match self.inner.get_image() {
            Ok(img) => {
                return Ok(Some(ClipboardMessage::Image {
                    width: img.width as u32,
                    height: img.height as u32,
                    rgba: img.bytes.into_owned(),
                }));
            }
            _ => {}
        }
        Ok(None)
    }

    async fn write(&mut self, msg: &ClipboardMessage) -> anyhow::Result<()> {
        match msg {
            ClipboardMessage::Text(text) => {
                self.inner.set_text(text.clone())?;
            }
            ClipboardMessage::Image { width, height, rgba } => {
                let img = ImageData {
                    width: *width as usize,
                    height: *height as usize,
                    bytes: Cow::Borrowed(rgba),
                };
                self.inner.set_image(img)?;
            }
            ClipboardMessage::Heartbeat => {}
        }
        Ok(())
    }
}
