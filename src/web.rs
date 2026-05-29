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
        
        /* Terminal Container styling */
        #terminal-container {
            width: 100%;
            height: 100%;
            display: none;
            background-color: #0b0f19;
            flex-grow: 1;
        }
        #terminal-container.active {
            display: block;
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

    <div id="terminal-container">
        <div id="terminal"></div>
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

    <!-- Xterm JS & Fit Addon -->
    <script src="https://cdn.jsdelivr.net/npm/xterm@5.3.0/lib/xterm.min.js"></script>
    <script src="https://cdn.jsdelivr.net/npm/xterm-addon-fit@0.8.0/lib/xterm-addon-fit.min.js"></script>

    <script>
        const authContainer = document.getElementById('auth-container');
        const terminalContainer = document.getElementById('terminal-container');
        const passwordInput = document.getElementById('password');
        const btnConnect = document.getElementById('btn-connect');
        const mobileToolbar = document.getElementById('mobile-toolbar');

        let ws;
        let term;
        let fitAddon;

        // Detect touch/mobile viewport
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
                // Use pointerdown for instant touch response
                btn.addEventListener('pointerdown', (e) => {
                    e.preventDefault();
                    sendKey(data);
                    if (term) term.focus();
                });
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
                const authMsg = {
                    type: "auth",
                    password: password,
                    session: "default",
                    cols: 80,
                    rows: 24
                };
                ws.send(JSON.stringify(authMsg));
            };

            ws.onmessage = (event) => {
                if (typeof event.data === 'string') {
                    const msg = JSON.parse(event.data);
                    if (msg.type === "auth_ok") {
                        authContainer.classList.add('hidden');
                        terminalContainer.classList.add('active');
                        
                        if (isMobile()) {
                            mobileToolbar.classList.add('active');
                        }
                        
                        initTerminal();
                    } else if (msg.type === "auth_fail") {
                        alert("Incorrect password!");
                        ws.close();
                    }
                } else {
                    if (term) {
                        term.write(new Uint8Array(event.data));
                    }
                }
            };

            ws.onclose = () => {
                authContainer.classList.remove('hidden');
                terminalContainer.classList.remove('active');
                mobileToolbar.classList.remove('active');
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

            // Bind mobile toolbar controls
            bindBtn('btn-esc', '\x1b');
            bindBtn('btn-tab', '\t');
            bindBtn('btn-prefix', '\x02'); // Ctrl+B
            bindBtn('btn-ctrlc', '\x03');  // Ctrl+C
            bindBtn('btn-ctrld', '\x04');  // Ctrl+D
            bindBtn('btn-detach', '\x07d'); // Ctrl-G + D (Detach)
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

        btnConnect.addEventListener('click', connect);
        passwordInput.addEventListener('keydown', (e) => {
            if (e.key === 'Enter') connect();
        });
    </script>
</body>
</html>
"#;
