use crossterm::{
    cursor,
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures_util::{SinkExt, StreamExt};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Terminal,
};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, Error, SignatureScheme};
use serde::{Deserialize, Serialize};
use std::io::{stdout, Write};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio_tungstenite::tungstenite::protocol::Message as WsMessage;

#[derive(Debug)]
struct NoOpServerCertVerifier;

impl ServerCertVerifier for NoOpServerCertVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ED25519,
        ]
    }
}

#[derive(Serialize)]
#[serde(tag = "type")]
enum ClientCommand {
    #[serde(rename = "auth_response")]
    AuthResponse {
        public_key: String, // hex client public key
        proof: String,      // hex proof hash
        session: Option<String>,
        cols: Option<u16>,
        rows: Option<u16>,
    },
    #[serde(rename = "resize")]
    Resize { cols: u16, rows: u16 },
    #[serde(rename = "list_sessions")]
    ListSessions,
    #[serde(rename = "attach")]
    Attach {
        session: String,
        cols: Option<u16>,
        rows: Option<u16>,
    },
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum ServerResponse {
    #[serde(rename = "auth_challenge")]
    AuthChallenge {
        salt: String,
        public_key: String, // hex server public key
    },
    #[serde(rename = "auth_ok")]
    AuthOk,
    #[serde(rename = "auth_fail")]
    AuthFail,
    #[serde(rename = "sessions_list")]
    SessionsList { sessions: Vec<String> },
}

enum DetachAction {
    Continue,
    Detach,
}

struct InputState {
    ctrl_g_pressed: bool,
}

pub async fn run_client(
    server_addr: SocketAddr,
    password: Option<String>,
    session: Option<String>,
) -> Result<(), anyhow::Error> {
    // 1. Get password
    let password = match password {
        Some(p) => p,
        None => {
            print!("Server Password: ");
            stdout().flush()?;

            // Read password securely with echo disabled
            let mut pass = String::new();
            enable_raw_mode()?;
            loop {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Enter => break,
                        KeyCode::Char(c) => pass.push(c),
                        KeyCode::Backspace => {
                            pass.pop();
                        }
                        KeyCode::Esc => {
                            disable_raw_mode()?;
                            println!();
                            return Ok(());
                        }
                        _ => {}
                    }
                }
            }
            disable_raw_mode()?;
            println!();
            pass
        }
    };

    // Install aws-lc-rs default provider
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    // 2. Setup TLS with Custom Verifier to accept self-signed certificates
    let client_config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoOpServerCertVerifier))
        .with_no_client_auth();

    let connector = tokio_tungstenite::Connector::Rustls(Arc::new(client_config));
    let ws_url = format!("wss://{}/ws", server_addr);

    println!("[Client] Connecting to {}...", ws_url);
    let (mut ws_stream, _) =
        tokio_tungstenite::connect_async_tls_with_config(ws_url, None, false, Some(connector))
            .await?;

    // Get current terminal size
    let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));

    // 3. Authenticate
    let challenge = match ws_stream.next().await {
        Some(Ok(WsMessage::Text(text))) => {
            let resp: ServerResponse = serde_json::from_str(&text)?;
            match resp {
                ServerResponse::AuthChallenge { salt, public_key } => (salt, public_key),
                _ => {
                    return Err(anyhow::anyhow!(
                        "Expected auth_challenge, got another response"
                    ))
                }
            }
        }
        _ => {
            return Err(anyhow::anyhow!(
                "Connection closed by server during challenge phase."
            ));
        }
    };
    let (salt, server_pub_hex) = challenge;

    let (client_priv, client_pub_bytes) = crate::crypto::generate_ecdh_keypair()?;
    let client_pub_hex: String = client_pub_bytes
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect();

    let server_pub_bytes = match hex_to_bytes(&server_pub_hex) {
        Ok(bytes) => bytes,
        Err(_) => return Err(anyhow::anyhow!("Invalid server public key received")),
    };

    let shared_secret = crate::crypto::derive_shared_secret(client_priv, &server_pub_bytes)?;
    let proof = crate::crypto::compute_auth_proof(&password, &shared_secret, &salt);

    let auth_cmd = ClientCommand::AuthResponse {
        public_key: client_pub_hex,
        proof,
        session: session.clone(),
        cols: Some(cols),
        rows: Some(rows),
    };
    ws_stream
        .send(WsMessage::Text(serde_json::to_string(&auth_cmd)?.into()))
        .await?;

    // Wait for auth reply
    let auth_resp = match ws_stream.next().await {
        Some(Ok(WsMessage::Text(text))) => {
            let resp: ServerResponse = serde_json::from_str(&text)?;
            resp
        }
        _ => {
            return Err(anyhow::anyhow!(
                "Connection closed by server during authentication response."
            ));
        }
    };

    match auth_resp {
        ServerResponse::AuthFail => {
            println!("Error: Incorrect password.");
            return Ok(());
        }
        ServerResponse::AuthOk => {
            println!("Connected and authenticated successfully!");
        }
        _ => return Err(anyhow::anyhow!("Invalid authentication response")),
    }

    // 4. Resolve session if not specified on CLI
    let final_session = match session {
        Some(s) => s,
        None => {
            // Request session list
            let list_cmd = ClientCommand::ListSessions;
            ws_stream
                .send(WsMessage::Text(serde_json::to_string(&list_cmd)?.into()))
                .await?;

            let sessions = match ws_stream.next().await {
                Some(Ok(WsMessage::Text(text))) => {
                    let resp: ServerResponse = serde_json::from_str(&text)?;
                    match resp {
                        ServerResponse::SessionsList { sessions } => sessions,
                        _ => Vec::new(),
                    }
                }
                _ => Vec::new(),
            };

            // Run Ratatui session selector
            match run_tui_selector(sessions)? {
                Some(s) => s,
                None => {
                    println!("Exiting...");
                    return Ok(());
                }
            }
        }
    };

    // Connect to the final session
    let attach_cmd = ClientCommand::Attach {
        session: final_session,
        cols: Some(cols),
        rows: Some(rows),
    };
    ws_stream
        .send(WsMessage::Text(serde_json::to_string(&attach_cmd)?.into()))
        .await?;

    // 5. Raw Terminal Loop
    println!("[Client] Entering raw terminal mode...");
    println!("Tip: Press Ctrl-G followed by D to detach.");
    std::thread::sleep(std::time::Duration::from_millis(500));

    enable_raw_mode()?;
    let mut stdout_handle = stdout();
    execute!(stdout_handle, EnterAlternateScreen)?;

    let (mut ws_write, mut ws_read) = ws_stream.split();
    let mut stdin = tokio::io::stdin();
    let mut input_state = InputState {
        ctrl_g_pressed: false,
    };
    let mut buf = [0u8; 1024];

    let mut event_stream = crossterm::event::EventStream::new();

    loop {
        tokio::select! {
            // Read keystrokes from local stdin
            n_res = stdin.read(&mut buf) => {
                let n = match n_res {
                    Ok(n) => n,
                    Err(_) => break,
                };
                if n == 0 { break; }

                // Inspect bytes for detach command (Ctrl-G + D)
                let mut clean_data = Vec::new();
                let mut action = DetachAction::Continue;

                for &byte in buf.iter().take(n) {
                    if input_state.ctrl_g_pressed {
                        input_state.ctrl_g_pressed = false;
                        if byte == b'd' || byte == b'D' {
                            action = DetachAction::Detach;
                            break;
                        } else if byte == 7 {
                            clean_data.push(7);
                        } else {
                            clean_data.push(7);
                            clean_data.push(byte);
                        }
                    } else if byte == 7 {
                        input_state.ctrl_g_pressed = true;
                    } else {
                        clean_data.push(byte);
                    }
                }

                if let DetachAction::Detach = action {
                    break;
                }

                if !clean_data.is_empty()
                    && ws_write.send(WsMessage::Binary(clean_data.into())).await.is_err()
                {
                    break;
                }
            }

            // Read output from remote tmux PTY
            ws_msg = ws_read.next() => {
                let msg = match ws_msg {
                    Some(Ok(m)) => m,
                    _ => break,
                };

                match msg {
                    WsMessage::Binary(data) => {
                        let _ = stdout_handle.write_all(&data);
                        let _ = stdout_handle.flush();
                    }
                    WsMessage::Close(_) => {
                        break;
                    }
                    _ => {}
                }
            }

            // Listen for local resize events
            crossterm_event = event_stream.next() => {
                if let Some(Ok(Event::Resize(w, h))) = crossterm_event {
                    let resize_cmd = ClientCommand::Resize { cols: w, rows: h };
                    let _ = ws_write.send(WsMessage::Text(serde_json::to_string(&resize_cmd).unwrap().into())).await;
                }
            }
        }
    }

    // Cleanup raw terminal state
    let _ = disable_raw_mode();
    let _ = execute!(stdout_handle, LeaveAlternateScreen);
    println!("\n[Client] Connection closed or session detached.");

    Ok(())
}

enum TuiMode {
    Select,
    NewSessionInput,
}

fn run_tui_selector(sessions: Vec<String>) -> Result<Option<String>, anyhow::Error> {
    enable_raw_mode()?;
    let mut stdout_handle = stdout();
    execute!(stdout_handle, EnterAlternateScreen, cursor::Hide)?;
    let backend = CrosstermBackend::new(stdout_handle);
    let mut terminal = Terminal::new(backend)?;

    let mut state = ListState::default();
    state.select(Some(0));

    let mut mode = TuiMode::Select;
    let mut new_session_name = String::new();
    let mut selected_session = None;

    loop {
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints(if let TuiMode::NewSessionInput = mode {
                    vec![
                        Constraint::Min(3),
                        Constraint::Length(3),
                        Constraint::Length(3),
                    ]
                } else {
                    vec![Constraint::Min(3), Constraint::Length(3)]
                })
                .split(f.area());

            // 1. Session list block
            let mut list_items: Vec<ListItem> = sessions
                .iter()
                .map(|s| ListItem::new(format!("  •  {}", s)))
                .collect();
            list_items.push(ListItem::new("  [+] Create New Session"));

            let list = List::new(list_items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Tailmux: Select Session ")
                        .border_style(Style::default().fg(Color::Rgb(99, 102, 241))),
                )
                .highlight_style(
                    Style::default()
                        .bg(Color::Rgb(79, 70, 229))
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol(" > ");

            f.render_stateful_widget(list, chunks[0], &mut state);

            // 2. Input box block (if in input mode)
            if let TuiMode::NewSessionInput = mode {
                let input_widget = Paragraph::new(new_session_name.as_str()).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" New Session Name ")
                        .border_style(Style::default().fg(Color::Rgb(167, 139, 250))),
                );
                f.render_widget(input_widget, chunks[1]);
            }

            // 3. Status bar
            let status_idx = if let TuiMode::NewSessionInput = mode {
                2
            } else {
                1
            };
            let status_text = match mode {
                TuiMode::Select => " [▲/▼] Navigate  [Enter] Select  [Q/Esc] Quit",
                TuiMode::NewSessionInput => {
                    " Type session name and press [Enter] to create, [Esc] to cancel"
                }
            };
            let status_bar = Paragraph::new(status_text)
                .block(Block::default().borders(Borders::NONE))
                .style(Style::default().fg(Color::Gray));
            f.render_widget(status_bar, chunks[status_idx]);
        })?;

        if let Event::Key(key) = event::read()? {
            match mode {
                TuiMode::Select => match key.code {
                    KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
                        break;
                    }
                    KeyCode::Up => {
                        let i = match state.selected() {
                            Some(i) => {
                                if i == 0 {
                                    sessions.len()
                                } else {
                                    i - 1
                                }
                            }
                            None => 0,
                        };
                        state.select(Some(i));
                    }
                    KeyCode::Down => {
                        let i = match state.selected() {
                            Some(i) => {
                                if i >= sessions.len() {
                                    0
                                } else {
                                    i + 1
                                }
                            }
                            None => 0,
                        };
                        state.select(Some(i));
                    }
                    KeyCode::Enter => {
                        if let Some(i) = state.selected() {
                            if i == sessions.len() {
                                // "[+] Create New Session" selected
                                mode = TuiMode::NewSessionInput;
                            } else {
                                selected_session = Some(sessions[i].clone());
                                break;
                            }
                        }
                    }
                    _ => {}
                },
                TuiMode::NewSessionInput => match key.code {
                    KeyCode::Esc => {
                        mode = TuiMode::Select;
                        new_session_name.clear();
                    }
                    KeyCode::Enter => {
                        let trimmed = new_session_name.trim();
                        if !trimmed.is_empty() {
                            selected_session = Some(trimmed.to_string());
                            break;
                        }
                    }
                    KeyCode::Char(c) => {
                        new_session_name.push(c);
                    }
                    KeyCode::Backspace => {
                        new_session_name.pop();
                    }
                    _ => {}
                },
            }
        }
    }

    disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen, cursor::Show)?;

    Ok(selected_session)
}

fn hex_to_bytes(s: &str) -> Result<Vec<u8>, anyhow::Error> {
    if !s.len().is_multiple_of(2) {
        return Err(anyhow::anyhow!("Odd length hex string"));
    }
    let mut bytes = Vec::with_capacity(s.len() / 2);
    for chunk in s.as_bytes().chunks(2) {
        let high = char::from(chunk[0])
            .to_digit(16)
            .ok_or_else(|| anyhow::anyhow!("Invalid hex char"))?;
        let low = char::from(chunk[1])
            .to_digit(16)
            .ok_or_else(|| anyhow::anyhow!("Invalid hex char"))?;
        bytes.push((high << 4 | low) as u8);
    }
    Ok(bytes)
}
