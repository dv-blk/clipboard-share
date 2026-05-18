use crate::payload::Payload;

#[cfg(target_os = "linux")]
mod wayland;

#[cfg(target_os = "windows")]
mod windows;

pub trait Clipboard: Clone + Send + Sync + 'static {
    fn changed(&self) -> impl std::future::Future<Output = anyhow::Result<Option<Payload>>> + Send;
    fn write(
        &self,
        payload: Payload,
    ) -> impl std::future::Future<Output = anyhow::Result<()>> + Send;
}

#[cfg(target_os = "linux")]
pub use wayland::WaylandClipboard;
#[cfg(target_os = "linux")]
pub type PlatformClipboard = WaylandClipboard;

#[cfg(target_os = "windows")]
pub use windows::WindowsClipboard;
#[cfg(target_os = "windows")]
pub type PlatformClipboard = WindowsClipboard;
