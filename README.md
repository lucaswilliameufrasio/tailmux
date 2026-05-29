# Tailmux

Tailmux is a lightweight, secure client-server terminal multiplexer written in Rust. It wraps **Tmux** to provide secure, low-latency remote access over a LAN or Tailscale without requiring SSH, featuring advanced session persistence across host reboots and independent client viewports.

## 🎯 Target Use Cases

Tailmux is particularly designed for:
- **Restricted Environments**: Network environments where outbound SSH ports (like port 22) are blocked, but TLS traffic (like HTTPS on 443 or custom ports) is permitted.
- **No-Terminal Clients**: Accessing your remote terminal workflows from devices that do not have terminal emulators or SSH clients installed (such as locked-down enterprise machines, Chromebooks, or mobile devices).
- **On-the-Go Mobile Access**: Quickly attaching to your active backend workspaces from your web browser on a smartphone or tablet to run diagnostics, execute commands, or check system logs.

---

## 🚀 Key Features

- **PTY Viewport Isolation (Grouped Tmux Sessions)**: In traditional tmux, attaching a small viewport (like a smartphone) and a large viewport (like a desktop monitor) to the same session forces tmux to resize the terminal to the smallest client, destroying the layout for both. Tailmux solves this by dynamically spawning a unique **grouped tmux session** per client (`tmux new-session -t <base_name> -s <client_sess>`). Both viewports share the same active windows and processes, but maintain their own independent cursor, window sizes, and active layouts without layout-resizing conflicts!
- **TLS 1.3 Encryption**: All communication is secured via TLS 1.3 using the FIPS-compliant `aws-lc-rs` cryptographic engine, securing your password and keystrokes from network sniffing.
- **Dynamic In-Memory Certificates**: Generates a self-signed TLS certificate in memory on startup, ensuring out-of-the-box encrypted traffic on local networks or Tailscale without manual SSL configuration.
- **Double Authentication**: Connections are protected by a random 6-character alphanumeric access pin generated on server boot, securing your console from unauthorized port scans.
- **Unified Web Console**: Serves a sleek, dark-themed HTML/JS landing page with [xterm.js](https://xtermjs.org/) to run your workspace entirely from any modern browser.
- **Interactive TUI Selector**: Connecting from the terminal CLI without arguments launches a [Ratatui](https://github.com/ratatui-org/ratatui)-based menu to list, navigate, select, or create sessions on the fly.
- **Reboot Persistence**: A daemon thread periodically writes base tmux session metadata (`cwd` and names) to `~/.config/tailmux/sessions.json`. After a system reboot, starting the server automatically resurrects your workspaces exactly where you left off.

---

## 🛠 Prerequisites

Ensure you have the Rust toolchain (`cargo`) and `tmux` installed on the host server.

```bash
sudo apt install tmux  # For Debian/Ubuntu
```

---

## 📦 Building from Source

To compile the optimized release binary, run:

```bash
cargo build --release
```

The executable binary will be generated at `./target/release/tailmux`.

---

## 📖 Usage Guide

### 1. Starting the Server (Remote Host)

Run the server subcommand. If you omit the `--password` option, Tailmux will generate a temporary 6-character access pin:

```bash
./target/release/tailmux server --bind 0.0.0.0:7788
```

Output:
```text
====================================================
  TAILMUX SERVER STARTED
  Address: 0.0.0.0:7788
  Access Password: A3E9X4
====================================================
```

*Note: You can specify a persistent password using `--password <your_password>`.*

### 2. Connecting via Terminal Client (Local Machine)

On your local client machine, run:

```bash
./target/release/tailmux client --connect <SERVER_IP>:7788
```

- **Password Prompt**: You will be securely prompted to input the server's access password (echoing is disabled).
- **Session List**: If no session is specified, the interactive TUI menu opens. Use `▲/▼` to navigate, `Enter` to attach, or type `N` to create a new workspace.
- **Direct Connect**: Bypass the TUI using the `--session` flag:
  ```bash
  ./target/release/tailmux client --connect <SERVER_IP>:7788 --session dev
  ```
- **Detaching**: To exit the terminal without terminating your remote processes, press **`Ctrl-g` followed by `d`**.

### 3. Connecting via Web Browser

Navigate to the server address in your web browser:

```text
https://<SERVER_IP>:7788/
```

1. Click **Advanced** and choose **Proceed/Accept Certificate** (this warning is normal since the server generates its self-signed certificate dynamically in memory).
2. Input the server's access password.
3. Your remote terminal is ready for browser interaction!
