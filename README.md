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

### 4. Manual Session Save & Restore (No Daemon Required)

Tailmux also allows you to save and restore your active tmux sessions manually without running the server daemon.

- **To save all active tmux sessions**:
  ```bash
  ./target/release/tailmux save
  ```
  This creates a snapshot of the session names and current working directories (`cwd`) to `~/.config/tailmux/sessions.json`.

- **To restore the saved sessions (e.g. after a reboot)**:
  ```bash
  ./target/release/tailmux restore
  ```
  This resurrects the saved tmux sessions with their original working directories, and automatically cleans up the JSON snapshot file.

---

## 🔒 Security Architecture & Authentication

Tailmux implements a hardened security design tailored for exposing terminal sessions on local or private networks (e.g., Tailscale):

1. **Zero-Knowledge Diffie-Hellman Authentication**:
   - Authentication is performed via an Elliptic Curve Diffie-Hellman (ECDH) key exchange over the P-256 curve (using `ring` in Rust and native Web Crypto APIs in the browser).
   - The password is never sent in plain text, even over TLS. Instead, both parties compute a shared secret. The client proves knowledge of the password by supplying a SHA-256 hash combination of the password, the shared secret, and an ephemeral server challenge salt.
   - Eavesdroppers cannot perform offline dictionary attacks to brute-force the password because they do not have the ephemeral ECDH private keys.

2. **Failed-Auth IP Rate Limiter**:
   - To protect the password from brute-force attempts while preventing false-positive blocks, rate limiting is only triggered by **failed authentication attempts** (not page refreshes or standard TCP connections).
   - If an IP address fails to authenticate **5 times within a 30-second window**, it is automatically banned for 5 minutes.
   - Banned IPs can be unbanned in real-time from the web administration panel.

3. **Access Password Persistence**:
   - The server saves the active password to `~/.config/tailmux/password.txt`.
   - Modifying the access password inside the Web Admin Panel saves it to disk automatically.
   - If you want to manually reset the password, edit or delete the `password.txt` file and restart the server.

---

## 🖥️ Running under Systemd

Running Tailmux as a background service ensures it starts automatically on boot and recovers from crashes. Since Tailmux wraps `tmux`, it should run under the user account that owns the active terminal sessions.

### Option A: System-Wide Service (Recommended)

1. Copy the provided `tailmux.service` file to systemd:
   ```bash
   sudo cp tailmux.service /etc/systemd/system/tailmux.service
   ```
2. Edit `/etc/systemd/system/tailmux.service` to change the `User=` line to match your username and configure the paths:
   ```ini
   [Service]
   User=your_username
   ExecStart=/home/your_username/.local/bin/tailmux server --bind [::]:7788
   ```
3. Reload systemd, enable, and start the daemon:
   ```bash
   sudo systemctl daemon-reload
   sudo systemctl enable tailmux
   sudo systemctl start tailmux
   ```

### Option B: User-Level Service (No Root Required)

If you don't have root access or want to run it purely within your user session:
1. Create the user systemd folder:
   ```bash
   mkdir -p ~/.config/systemd/user/
   ```
2. Copy `tailmux.service` into it, making sure to remove the `User=` configuration line since user services run under the parent session user:
   ```bash
   cp tailmux.service ~/.config/systemd/user/tailmux.service
   # Remove User= line from the user service file:
   sed -i '/User=/d' ~/.config/systemd/user/tailmux.service
   ```
3. Start and enable the service:
   ```bash
   systemctl --user daemon-reload
   systemctl --user enable tailmux
   systemctl --user start tailmux
   ```
4. To ensure the service continues running after you log out:
   ```bash
   sudo loginctl enable-linger $USER
   ```

