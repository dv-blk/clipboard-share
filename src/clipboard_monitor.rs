use tokio::sync::watch;
use tracing::{debug, error, info};

use crate::echo_guard::EchoGuard;
use crate::message::ClipboardMessage;

/// Monitors the local clipboard for changes and publishes them to a
/// `watch` channel. Runs independently of any network connection, so
/// clipboard changes are captured immediately and sent as soon as a
/// peer connects.
///
/// Platform behaviour:
/// - **Linux/Wayland**: event-driven via `wl-clipboard-rs`. Blocks on the
///   compositor's `selection` event — zero CPU usage between copies.
/// - **Windows**: polls via `arboard` every `poll_interval_ms` milliseconds.
///
/// Echo suppression: before publishing, the fingerprint of the new content
/// is checked against `EchoGuard`. If it matches the content most recently
/// written from the peer, the update is suppressed to prevent echo loops.
pub struct ClipboardMonitor {
    echo_guard: EchoGuard,
    tx: watch::Sender<Option<ClipboardMessage>>,
}

impl ClipboardMonitor {
    /// Create a new monitor and return it along with the watch receiver that
    /// consumers (i.e. `SyncEngine`) should subscribe to.
    pub fn new(echo_guard: EchoGuard) -> (Self, watch::Receiver<Option<ClipboardMessage>>) {
        let (tx, rx) = watch::channel(None);
        (Self { echo_guard, tx }, rx)
    }

    /// Start the monitor. This spawns a background task and returns immediately.
    /// The task runs for the lifetime of the process.
    pub fn start(self) {
        #[cfg(target_os = "linux")]
        tokio::spawn(run_wayland(self.echo_guard, self.tx));

        #[cfg(target_os = "windows")]
        tokio::spawn(run_windows(self.echo_guard, self.tx));
    }
}

// ── Linux / Wayland ─────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
async fn run_wayland(
    echo_guard: EchoGuard,
    tx: watch::Sender<Option<ClipboardMessage>>,
) {
    use crate::clipboard::wayland::blocking_wait_for_change;

    info!("clipboard monitor started (Wayland event-driven)");

    let mut last_fp: Option<Vec<u8>> = None;

    loop {
        // Run the blocking Wayland event wait on a dedicated thread so we
        // don't stall the async runtime.
        let result = tokio::task::spawn_blocking(blocking_wait_for_change).await;

        match result {
            Err(e) => {
                error!("clipboard monitor task panicked: {e}");
                break;
            }
            Ok(Err(e)) => {
                error!("clipboard monitor error: {e}");
                // Brief pause to avoid a tight error loop, then retry.
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
            Ok(Ok(None)) => {
                debug!("clipboard monitor: selection event with no supported content");
            }
            Ok(Ok(Some(msg))) => {
                let fp = msg.fingerprint();
                if echo_guard.is_echo(&fp) {
                    debug!("clipboard monitor: suppressing echo");
                } else if last_fp.as_deref() == Some(fp.as_slice()) {
                    debug!("clipboard monitor: content unchanged, skipping");
                } else {
                    info!("clipboard monitor: new content detected, publishing");
                    last_fp = Some(fp);
                    let _ = tx.send(Some(msg));
                }
            }
        }
    }
}

// ── Windows ──────────────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
async fn run_windows(
    echo_guard: EchoGuard,
    tx: watch::Sender<Option<ClipboardMessage>>,
) {
    use crate::clipboard::windows::WindowsClipboard;

    info!("clipboard monitor started (Windows event-driven)");

    // Bridge: Win32 message thread → tokio watch channel.
    // std::sync::mpsc is used because the message loop runs on a raw OS thread.
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<()>();

    // Spawn a dedicated OS thread to own the Win32 message loop.
    std::thread::spawn(move || {
        win32_clipboard_listener(event_tx);
    });

    // Open arboard once for the lifetime of the monitor.
    let mut clipboard = match WindowsClipboard::new() {
        Ok(c) => c,
        Err(e) => {
            error!("failed to open clipboard: {e}");
            return;
        }
    };

    let mut last_fp: Option<Vec<u8>> = None;

    // Each WM_CLIPBOARDUPDATE notification arrives as a () signal.
    while event_rx.recv().await.is_some() {
        let msg = match clipboard.read_blocking() {
            Ok(Some(m)) => m,
            Ok(None) => continue,
            Err(e) => {
                error!("clipboard read error: {e}");
                continue;
            }
        };

        let fp = msg.fingerprint();

        if echo_guard.is_echo(&fp) {
            debug!("clipboard monitor: suppressing echo");
            continue;
        }

        if last_fp.as_deref() == Some(fp.as_slice()) {
            debug!("clipboard monitor: content unchanged, skipping");
            continue;
        }

        info!("clipboard monitor: new content detected, publishing");
        last_fp = Some(fp);
        let _ = tx.send(Some(msg));
    }
}

/// Creates a hidden message-only window, registers for clipboard change
/// notifications, and runs a Win32 message loop. Sends a `()` signal on
/// `event_tx` for every `WM_CLIPBOARDUPDATE` message received.
///
/// This function blocks indefinitely and is intended to run on a dedicated
/// OS thread.
#[cfg(target_os = "windows")]
fn win32_clipboard_listener(event_tx: tokio::sync::mpsc::UnboundedSender<()>) {
    use windows::Win32::System::DataExchange::AddClipboardFormatListener;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, DispatchMessageW, GetMessageW,
        RegisterClassW, TranslateMessage, HWND_MESSAGE, MSG,
        WNDCLASSW, WS_EX_NOACTIVATE, WS_OVERLAPPED,
    };
    use windows::core::PCWSTR;

    unsafe {
        // Register a minimal window class.
        let class_name: Vec<u16> = "clipboard-share-listener\0"
            .encode_utf16()
            .collect();

        // We need a way to pass event_tx into the window procedure. We use a
        // thread-local since Win32 WNDPROCs don't have a user-data pointer in
        // this simplified approach.
        CLIPBOARD_TX.with(|cell| {
            *cell.borrow_mut() = Some(event_tx);
        });

        let wc = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };

        RegisterClassW(&wc);

        // Create a message-only window (invisible, no taskbar entry).
        let hwnd = CreateWindowExW(
            WS_EX_NOACTIVATE,
            PCWSTR(class_name.as_ptr()),
            PCWSTR::null(),
            WS_OVERLAPPED,
            0, 0, 0, 0,
            Some(HWND_MESSAGE),
            None,
            None,
            None,
        );

        if hwnd.is_err() {
            tracing::error!("failed to create clipboard listener window");
            return;
        }
        let hwnd = hwnd.unwrap();

        // Register for WM_CLIPBOARDUPDATE notifications.
        if AddClipboardFormatListener(hwnd).is_err() {
            tracing::error!("AddClipboardFormatListener failed");
            DestroyWindow(hwnd).ok();
            return;
        }

        // Standard Win32 message loop.
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

#[cfg(target_os = "windows")]
std::thread_local! {
    static CLIPBOARD_TX: std::cell::RefCell<Option<tokio::sync::mpsc::UnboundedSender<()>>> =
        std::cell::RefCell::new(None);
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn wnd_proc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, PostQuitMessage, WM_CLIPBOARDUPDATE, WM_DESTROY,
    };
    match msg {
        WM_CLIPBOARDUPDATE => {
            CLIPBOARD_TX.with(|cell| {
                if let Some(tx) = cell.borrow().as_ref() {
                    let _ = tx.send(());
                }
            });
            windows::Win32::Foundation::LRESULT(0)
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            windows::Win32::Foundation::LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
