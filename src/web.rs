pub const INDEX_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
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
        }
        #terminal-container.active {
            display: block;
        }
        #terminal {
            width: 100%;
            height: 100%;
            padding: 10px;
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

    <!-- Xterm JS & Fit Addon -->
    <script src="https://cdn.jsdelivr.net/npm/xterm@5.3.0/lib/xterm.min.js"></script>
    <script src="https://cdn.jsdelivr.net/npm/xterm-addon-fit@0.8.0/lib/xterm-addon-fit.min.js"></script>

    <script>
        const authContainer = document.getElementById('auth-container');
        const terminalContainer = document.getElementById('terminal-container');
        const passwordInput = document.getElementById('password');
        const btnConnect = document.getElementById('btn-connect');

        let ws;
        let term;
        let fitAddon;

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
