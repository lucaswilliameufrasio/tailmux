pub const INDEX_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no">
    <title>Tailmux Web Console</title>
    <!-- Xterm CSS -->
    <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/xterm@5.3.0/css/xterm.min.css" />
    <style>
        * {
            box-sizing: border-box;
            margin: 0;
            padding: 0;
        }
        body, html {
            width: 100%;
            height: 100%;
            background-color: #0b0f19;
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
            color: #f3f4f6;
            overflow: hidden;
            display: flex;
            flex-direction: column;
            justify-content: center;
            align-items: center;
        }
        
        /* Auth Modal styling */
        #auth-container {
            background: rgba(17, 24, 39, 0.7);
            backdrop-filter: blur(12px);
            border: 1px solid rgba(255, 255, 255, 0.1);
            border-radius: 16px;
            padding: 2.5rem;
            width: 90%;
            max-width: 420px;
            text-align: center;
            box-shadow: 0 10px 25px -5px rgba(0, 0, 0, 0.5), 0 8px 10px -6px rgba(0, 0, 0, 0.5);
            transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
            z-index: 100;
        }
        #auth-container.hidden {
            opacity: 0;
            transform: scale(0.95);
            pointer-events: none;
            display: none;
        }
        h2 {
            font-size: 1.75rem;
            margin-bottom: 0.5rem;
            font-weight: 700;
            background: linear-gradient(135deg, #a78bfa 0%, #6366f1 100%);
            -webkit-background-clip: text;
            -webkit-text-fill-color: transparent;
        }
        p.subtitle {
            font-size: 0.875rem;
            color: #9ca3af;
            margin-bottom: 2rem;
        }
        .input-group {
            margin-bottom: 1.5rem;
            text-align: left;
        }
        label {
            display: block;
            font-size: 0.75rem;
            text-transform: uppercase;
            letter-spacing: 0.05em;
            color: #9ca3af;
            margin-bottom: 0.5rem;
            font-weight: 600;
        }
        input {
            width: 100%;
            padding: 0.75rem 1rem;
            background: rgba(31, 41, 55, 0.5);
            border: 1px solid rgba(255, 255, 255, 0.1);
            border-radius: 8px;
            color: #fff;
            font-size: 1rem;
            outline: none;
            transition: border-color 0.2s;
        }
        input:focus {
            border-color: #6366f1;
            box-shadow: 0 0 0 2px rgba(99, 102, 241, 0.2);
        }
        button {
            width: 100%;
            padding: 0.75rem;
            background: linear-gradient(135deg, #8b5cf6 0%, #6366f1 100%);
            border: none;
            border-radius: 8px;
            color: #fff;
            font-size: 1rem;
            font-weight: 600;
            cursor: pointer;
            transition: opacity 0.2s;
        }
        button:hover {
            opacity: 0.9;
        }
        
        /* Layout wrapping terminal and header */
        #main-layout {
            display: none;
            width: 100%;
            height: 100%;
            flex-direction: column;
            background-color: #0b0f19;
        }
        #main-layout.active {
            display: flex;
        }
        
        /* Top Navigation Header */
        #nav-header {
            display: flex;
            justify-content: space-between;
            align-items: center;
            background: rgba(17, 24, 39, 0.8);
            border-bottom: 1px solid rgba(255, 255, 255, 0.05);
            padding: 10px 20px;
            width: 100%;
            height: 50px;
        }
        #logo-title {
            font-weight: 700;
            font-size: 1.1rem;
            background: linear-gradient(135deg, #a78bfa 0%, #6366f1 100%);
            -webkit-background-clip: text;
            -webkit-text-fill-color: transparent;
        }
        .header-btn {
            background: rgba(255, 255, 255, 0.08);
            border: 1px solid rgba(255, 255, 255, 0.1);
            border-radius: 6px;
            color: #f3f4f6;
            padding: 6px 12px;
            font-size: 0.85rem;
            font-weight: 600;
            cursor: pointer;
            transition: background 0.2s;
        }
        .header-btn:hover {
            background: rgba(255, 255, 255, 0.15);
        }

        #terminal-container {
            width: 100%;
            flex-grow: 1;
            overflow: hidden;
        }
        #terminal {
            width: 100%;
            height: 100%;
            padding: 10px;
        }

        /* Mobile Toolbar Styling */
        #mobile-toolbar {
            display: none;
            background: rgba(17, 24, 39, 0.9);
            border-top: 1px solid rgba(255, 255, 255, 0.1);
            padding: 8px;
            gap: 8px;
            width: 100%;
            overflow-x: auto;
            white-space: nowrap;
            -webkit-overflow-scrolling: touch;
            z-index: 10;
        }
        #mobile-toolbar.active {
            display: flex;
        }
        .toolbar-btn {
            background: rgba(255, 255, 255, 0.08);
            border: 1px solid rgba(255, 255, 255, 0.1);
            border-radius: 6px;
            color: #f3f4f6;
            padding: 8px 14px;
            font-size: 0.85rem;
            font-weight: 600;
            cursor: pointer;
            user-select: none;
            display: inline-block;
            touch-action: manipulation;
        }
        .toolbar-btn:active {
            background: #8b5cf6;
            color: #fff;
            border-color: #a78bfa;
        }

        /* Admin Overlay Modal */
        #admin-overlay {
            display: none;
            position: fixed;
            top: 0;
            left: 0;
            width: 100%;
            height: 100%;
            background: rgba(0, 0, 0, 0.65);
            backdrop-filter: blur(8px);
            z-index: 200;
            justify-content: center;
            align-items: center;
        }
        #admin-overlay.active {
            display: flex;
        }
        #admin-modal {
            background: #111827;
            border: 1px solid rgba(255, 255, 255, 0.15);
            border-radius: 12px;
            width: 90%;
            max-width: 600px;
            max-height: 85%;
            overflow-y: auto;
            padding: 1.5rem;
            box-shadow: 0 20px 25px -5px rgba(0, 0, 0, 0.6);
        }
        .admin-header {
            display: flex;
            justify-content: space-between;
            align-items: center;
            border-bottom: 1px solid rgba(255, 255, 255, 0.1);
            padding-bottom: 10px;
            margin-bottom: 20px;
        }
        .admin-section {
            margin-bottom: 25px;
        }
        .admin-section h3 {
            font-size: 1rem;
            margin-bottom: 10px;
            color: #a78bfa;
            text-transform: uppercase;
            letter-spacing: 0.05em;
        }
        .admin-table {
            width: 100%;
            border-collapse: collapse;
            text-align: left;
            margin-top: 5px;
            font-size: 0.9rem;
        }
        .admin-table th, .admin-table td {
            padding: 8px 12px;
            border-bottom: 1px solid rgba(255, 255, 255, 0.05);
        }
        .admin-table th {
            color: #9ca3af;
            font-weight: 600;
        }
        .action-btn {
            background: #ef4444;
            color: white;
            border: none;
            padding: 4px 8px;
            font-size: 0.75rem;
            border-radius: 4px;
            cursor: pointer;
            font-weight: 600;
        }
        .action-btn.unban {
            background: #10b981;
        }
        .action-btn:hover {
            opacity: 0.9;
        }
        .admin-pw-row {
            display: flex;
            gap: 10px;
            margin-top: 5px;
        }
        .admin-pw-row input {
            flex-grow: 1;
            padding: 6px 10px;
            font-size: 0.9rem;
        }
        .admin-pw-row button {
            width: auto;
            padding: 6px 15px;
            font-size: 0.9rem;
        }
    </style>
</head>
<body>

    <div id="auth-container">
        <h2>Tailmux Console</h2>
        <p class="subtitle">Enter the server password to connect</p>
        <div class="input-group">
            <label for="password">Access Password</label>
            <input type="password" id="password" placeholder="Type password..." autofocus />
        </div>
        <button id="btn-connect">Connect</button>
    </div>

    <!-- Main Layout replacing terminal container -->
    <div id="main-layout">
        <div id="nav-header">
            <span id="logo-title">Tailmux Console</span>
            <button class="header-btn" id="btn-admin-toggle">Admin Panel</button>
        </div>
        <div id="terminal-container">
            <div id="terminal"></div>
        </div>
    </div>

    <div id="mobile-toolbar">
        <button class="toolbar-btn" id="btn-esc">ESC</button>
        <button class="toolbar-btn" id="btn-tab">TAB</button>
        <button class="toolbar-btn" id="btn-prefix">Ctrl+B</button>
        <button class="toolbar-btn" id="btn-ctrlc">Ctrl+C</button>
        <button class="toolbar-btn" id="btn-ctrld">Ctrl+D</button>
        <button class="toolbar-btn" id="btn-detach">Detach</button>
        <button class="toolbar-btn" id="btn-left">◀</button>
        <button class="toolbar-btn" id="btn-up">▲</button>
        <button class="toolbar-btn" id="btn-down">▼</button>
        <button class="toolbar-btn" id="btn-right">▶</button>
    </div>

    <!-- Admin Overlay -->
    <div id="admin-overlay">
        <div id="admin-modal">
            <div class="admin-header">
                <h3>Server Administration</h3>
                <button class="header-btn" id="btn-admin-close">Close</button>
            </div>
            
            <div class="admin-section">
                <h3>Change Access Password</h3>
                <div class="admin-pw-row">
                    <input type="text" id="admin-password-input" placeholder="New server password..." />
                    <button class="toolbar-btn" id="btn-admin-change-pw">Update</button>
                </div>
            </div>

            <div class="admin-section">
                <h3>Active Connections</h3>
                <table class="admin-table" id="connections-table">
                    <thead>
                        <tr>
                            <th>Connection ID</th>
                            <th>IP Address</th>
                        </tr>
                    </thead>
                    <tbody>
                        <!-- Populated dynamically -->
                    </tbody>
                </table>
            </div>

            <div class="admin-section">
                <h3>Banned IPs</h3>
                <table class="admin-table" id="banned-ips-table">
                    <thead>
                        <tr>
                            <th>IP Address</th>
                            <th>Banned For</th>
                            <th>Actions</th>
                        </tr>
                    </thead>
                    <tbody>
                        <!-- Populated dynamically -->
                    </tbody>
                </table>
            </div>
        </div>
    </div>

    <!-- Xterm JS & Fit Addon -->
    <script src="https://cdn.jsdelivr.net/npm/xterm@5.3.0/lib/xterm.min.js"></script>
    <script src="https://cdn.jsdelivr.net/npm/xterm-addon-fit@0.8.0/lib/xterm-addon-fit.min.js"></script>

    <script>
        const authContainer = document.getElementById('auth-container');
        const mainLayout = document.getElementById('main-layout');
        const passwordInput = document.getElementById('password');
        const btnConnect = document.getElementById('btn-connect');
        const mobileToolbar = document.getElementById('mobile-toolbar');
        const adminOverlay = document.getElementById('admin-overlay');
        const btnAdminToggle = document.getElementById('btn-admin-toggle');
        const btnAdminClose = document.getElementById('btn-admin-close');
        
        const connectionsTable = document.getElementById('connections-table').querySelector('tbody');
        const bannedIpsTable = document.getElementById('banned-ips-table').querySelector('tbody');
        const adminPasswordInput = document.getElementById('admin-password-input');
        const btnAdminChangePw = document.getElementById('btn-admin-change-pw');

        let ws;
        let term;
        let fitAddon;

        function isMobile() {
            return window.innerWidth <= 768 || ('ontouchstart' in window) || (navigator.maxTouchPoints > 0);
        }

        function sendKey(data) {
            if (ws && ws.readyState === WebSocket.OPEN) {
                const encoder = new TextEncoder();
                ws.send(encoder.encode(data));
            }
        }

        function bindBtn(id, data) {
            const btn = document.getElementById(id);
            if (btn) {
                btn.addEventListener('pointerdown', (e) => {
                    e.preventDefault();
                    sendKey(data);
                    if (term) term.focus();
                });
            }
        }

        function hexToBytes(hexString) {
            if (hexString.length % 2 !== 0) {
                throw new Error("Invalid hex string");
            }
            const bytes = new Uint8Array(hexString.length / 2);
            for (let i = 0; i < hexString.length; i += 2) {
                bytes[i / 2] = parseInt(hexString.substring(i, i + 2), 16);
            }
            return bytes.buffer;
        }

        function bytesToHex(buffer) {
            return Array.from(new Uint8Array(buffer))
                .map(b => b.toString(16).padStart(2, '0'))
                .join('');
        }

        async function handleChallenge(challengeMsg, password) {
            try {
                const serverPubKeyBytes = hexToBytes(challengeMsg.public_key);
                
                // 1. Generate client ECDH ephemeral keypair
                const clientKeyPair = await window.crypto.subtle.generateKey(
                    {
                        name: "ECDH",
                        namedCurve: "P-256"
                    },
                    true,
                    ["deriveKey", "deriveBits"]
                );

                // 2. Export client public key to raw format
                const clientPubRaw = await window.crypto.subtle.exportKey("raw", clientKeyPair.publicKey);
                const clientPubHex = bytesToHex(clientPubRaw);

                // 3. Import server public key
                const serverPubKey = await window.crypto.subtle.importKey(
                    "raw",
                    serverPubKeyBytes,
                    {
                        name: "ECDH",
                        namedCurve: "P-256"
                    },
                    true,
                    []
                );

                // 4. Derive shared secret (256 bits = 32 bytes)
                const sharedBits = await window.crypto.subtle.deriveBits(
                    {
                        name: "ECDH",
                        public: serverPubKey
                    },
                    clientKeyPair.privateKey,
                    256
                );
                
                const sharedSecretHex = bytesToHex(sharedBits);

                // 5. Compute proofInput: password + ":" + sharedSecretHex + ":" + salt + ":client"
                const proofInput = password + ":" + sharedSecretHex + ":" + challengeMsg.salt + ":client";
                const encoder = new TextEncoder();
                const proofInputBytes = encoder.encode(proofInput);

                // 6. Compute SHA-256 of proofInput
                const hashBuffer = await window.crypto.subtle.digest("SHA-256", proofInputBytes);
                const proofHex = bytesToHex(hashBuffer);

                // 7. Send AuthResponse
                const authMsg = {
                    type: "auth_response",
                    public_key: clientPubHex,
                    proof: proofHex,
                    session: "default",
                    cols: 80,
                    rows: 24
                };
                ws.send(JSON.stringify(authMsg));
            } catch (err) {
                console.error("DH Key Exchange / Auth failed:", err);
                alert("Failed to compute secure authentication. Check console for details.");
                ws.close();
            }
        }

        function connect() {
            const password = passwordInput.value;
            if (!password) {
                alert('Password is required!');
                return;
            }

            const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
            const wsUrl = `${protocol}//${window.location.host}/ws`;

            ws = new WebSocket(wsUrl);
            ws.binaryType = 'arraybuffer';

            ws.onopen = () => {
                // Connection open - wait for server challenge
            };

            ws.onmessage = (event) => {
                if (typeof event.data === 'string') {
                    const msg = JSON.parse(event.data);
                    if (msg.type === "auth_challenge") {
                        handleChallenge(msg, password);
                    } else if (msg.type === "auth_ok") {
                        authContainer.classList.add('hidden');
                        mainLayout.classList.add('active');
                        
                        if (isMobile()) {
                            mobileToolbar.classList.add('active');
                        }
                        
                        initTerminal();
                    } else if (msg.type === "auth_fail") {
                        alert("Incorrect password or authentication failed!");
                        ws.close();
                    } else if (msg.type === "admin_status") {
                        renderAdminStatus(msg);
                    } else if (msg.type === "admin_password_changed") {
                        alert("Password updated successfully!");
                    }
                } else {
                    if (term) {
                        term.write(new Uint8Array(event.data));
                    }
                }
            };

            ws.onclose = () => {
                authContainer.classList.remove('hidden');
                mainLayout.classList.remove('active');
                mobileToolbar.classList.remove('active');
                adminOverlay.classList.remove('active');
                if (term) {
                    term.dispose();
                    term = null;
                }
            };

            ws.onerror = (err) => {
                console.error("WebSocket error:", err);
            };
        }

        function initTerminal() {
            term = new Terminal({
                cursorBlink: true,
                fontFamily: 'Consolas, Monaco, "Andale Mono", "Ubuntu Mono", monospace',
                fontSize: 14,
                theme: {
                    background: '#0b0f19',
                    foreground: '#f3f4f6',
                    cursor: '#8b5cf6',
                    selectionBackground: 'rgba(99, 102, 241, 0.3)'
                }
            });

            fitAddon = new FitAddon.FitAddon();
            term.loadAddon(fitAddon);
            term.open(document.getElementById('terminal'));
            fitAddon.fit();

            sendResize();

            term.onData(data => {
                const encoder = new TextEncoder();
                const binaryData = encoder.encode(data);
                ws.send(binaryData);
            });

            bindBtn('btn-esc', '\x1b');
            bindBtn('btn-tab', '\t');
            bindBtn('btn-prefix', '\x02');
            bindBtn('btn-ctrlc', '\x03');
            bindBtn('btn-ctrld', '\x04');
            bindBtn('btn-detach', '\x07d');
            bindBtn('btn-up', '\x1b[A');
            bindBtn('btn-down', '\x1b[B');
            bindBtn('btn-left', '\x1b[D');
            bindBtn('btn-right', '\x1b[C');

            window.addEventListener('resize', () => {
                if (fitAddon) {
                    fitAddon.fit();
                    sendResize();
                }
            });
        }

        function sendResize() {
            if (ws && ws.readyState === WebSocket.OPEN) {
                const resizeMsg = {
                    type: "resize",
                    cols: term.cols,
                    rows: term.rows
                };
                ws.send(JSON.stringify(resizeMsg));
            }
        }

        // Admin Panel Functions
        btnAdminToggle.addEventListener('click', () => {
            if (ws && ws.readyState === WebSocket.OPEN) {
                ws.send(JSON.stringify({ type: "admin_get_status" }));
                adminOverlay.classList.add('active');
            }
        });

        btnAdminClose.addEventListener('click', () => {
            adminOverlay.classList.remove('active');
            if (term) term.focus();
        });

        btnAdminChangePw.addEventListener('click', () => {
            const newPw = adminPasswordInput.value.trim();
            if (newPw) {
                if (confirm("Are you sure you want to change the server password? New connections will require the new password.")) {
                    ws.send(JSON.stringify({
                        type: "admin_change_password",
                        new_password: newPw
                    }));
                }
            } else {
                alert("Password cannot be empty.");
            }
        });

        function renderAdminStatus(data) {
            // Render current password
            adminPasswordInput.value = data.current_password;

            // Render Connections table
            connectionsTable.innerHTML = '';
            if (data.connections.length === 0) {
                connectionsTable.innerHTML = '<tr><td colspan="2" style="text-align: center; color: #9ca3af;">No active connections</td></tr>';
            } else {
                data.connections.forEach(conn => {
                    const row = document.createElement('tr');
                    row.innerHTML = `
                        <td><code>${conn.id}</code></td>
                        <td>${conn.addr}</td>
                    `;
                    connectionsTable.appendChild(row);
                });
            }

            // Render Banned IPs table
            bannedIpsTable.innerHTML = '';
            if (data.banned_ips.length === 0) {
                bannedIpsTable.innerHTML = '<tr><td colspan="3" style="text-align: center; color: #9ca3af;">No banned IPs</td></tr>';
            } else {
                data.banned_ips.forEach(ban => {
                    const row = document.createElement('tr');
                    
                    const min = Math.floor(ban.expires_in_secs / 60);
                    const sec = ban.expires_in_secs % 60;
                    const timeLeft = `${min}m ${sec}s`;

                    row.innerHTML = `
                        <td><code>${ban.ip}</code></td>
                        <td>${timeLeft}</td>
                        <td><button class="action-btn unban" data-ip="${ban.ip}">Unban</button></td>
                    `;
                    bannedIpsTable.appendChild(row);
                });

                // Attach unban listeners
                bannedIpsTable.querySelectorAll('.unban').forEach(btn => {
                    btn.addEventListener('click', (e) => {
                        const ip = e.target.getAttribute('data-ip');
                        ws.send(JSON.stringify({
                            type: "admin_unban_ip",
                            ip: ip
                        }));
                    });
                });
            }
        }

        btnConnect.addEventListener('click', connect);
        passwordInput.addEventListener('keydown', (e) => {
            if (e.key === 'Enter') connect();
        });
    </script>
</body>
</html>
"#;
