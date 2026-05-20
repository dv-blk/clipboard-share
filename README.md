# clipboard-share

Bidirectional clipboard sync over TCP. Copy on one machine, paste on another.

Supports Linux (Wayland), Windows, and Android (receive-only).

## Download

Pre-built binaries and APK are available on the [Releases](https://github.com/dv-blk/clipboard-share/releases) page.

## Usage

Run on each machine, pointing at the other's address:

```bash
# Machine A
clipboard-share --listen 0.0.0.0:9876 --peer 192.168.1.20:9876

# Machine B
clipboard-share --listen 0.0.0.0:9876 --peer 192.168.1.10:9876
```

Multiple peers are supported:

```bash
clipboard-share --listen 0.0.0.0:9876 --peer 192.168.1.20:9876 --peer 192.168.1.30:9876
```

The two sides will connect to each other regardless of which initiates first. Reconnection is automatic.

### Android

Install the APK and start the service from the app. Configure the port to match your peers and it will receive clipboard updates from any connected peer.

### Options

| Flag | Default | Description |
|---|---|---|
| `--listen` | `0.0.0.0:9876` | Address to listen on |
| `--peer` | *(required)* | Peer address(es) to connect to |
| `--reconnect-delay-ms` | `4000` | Delay between reconnect attempts |

## Running at startup

### Linux (systemd user service)

Create `~/.config/systemd/user/clipboard-share.service`:

```ini
[Unit]
Description=Clipboard Share

[Service]
ExecStart=/path/to/clipboard-share --listen 0.0.0.0:9876 --peer 192.168.1.20:9876
Restart=on-failure
RestartSec=4s

[Install]
WantedBy=default.target
```

Then enable and start it:

```bash
systemctl --user enable --now clipboard-share
```

### Windows (startup shortcut)

1. Press `Win+R`, type `shell:startup`, press Enter
2. Right-click in the folder → **New → Shortcut**
3. Set the target to the full path of the binary with your arguments, e.g.:
   ```
   C:\path\to\clipboard-share.exe --listen 0.0.0.0:9876 --peer 192.168.1.20:9876
   ```
4. Click **Finish**

The process runs silently in the background with no console window.

## Building

**Linux:**
```bash
cargo build --release
```

**Windows (cross-compile from Linux):**
```bash
cargo build --release --target x86_64-pc-windows-gnu
```

**Android:** open the `android/` directory in Android Studio and generate a signed APK.
