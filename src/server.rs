use axum::{
    extract::ws::{Message as AxumMessage, WebSocket, WebSocketUpgrade},
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder as ConnBuilder;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::{Instant, Duration};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio_rustls::TlsAcceptor;
use std::sync::Arc;

use crate::crypto::generate_self_signed_config;
use crate::web::INDEX_HTML;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SavedSession {
    pub name: String,
    pub cwd: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SavedState {
    pub sessions: Vec<SavedSession>,
}

pub struct ConnectionTracker {
    pub timestamps: Vec<Instant>,
    pub banned_until: Option<Instant>,
}

pub struct ServerState {
    pub password: String,
    pub connection_history: Mutex<HashMap<String, ConnectionTracker>>,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum ClientCommand {
    #[serde(rename = "auth")]
    Auth {
        password: String,
        session: Option<String>,
        cols: Option<u16>,
        rows: Option<u16>,
    },
    #[serde(rename = "resize")]
    Resize {
        cols: u16,
        rows: u16,
    },
    #[serde(rename = "list_sessions")]
    ListSessions,
    #[serde(rename = "attach")]
    Attach {
        session: String,
        cols: Option<u16>,
        rows: Option<u16>,
    },
}

#[derive(Serialize)]
#[serde(tag = "type")]
enum ServerResponse {
    #[serde(rename = "auth_ok")]
    AuthOk,
    #[serde(rename = "auth_fail")]
    AuthFail,
    #[serde(rename = "sessions_list")]
    SessionsList { sessions: Vec<String> },
}

pub async fn run_server(bind_addr: SocketAddr, password: String) -> Result<(), anyhow::Error> {
    // 1. Restore previous tmux sessions if they were saved before reboot
    if let Err(e) = restore_sessions().await {
        eprintln!("[Server] Error restoring tmux sessions: {:?}", e);
    }

    // 2. Setup server state
    let server_state = Arc::new(ServerState {
        password,
        connection_history: Mutex::new(HashMap::new()),
    });

    // 3. Start a background task to periodically save tmux sessions for reboot persistence
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
            if let Err(e) = save_sessions_state().await {
                eprintln!("[Server] Error saving persistence state: {:?}", e);
            }
        }
    });

    // 4. Setup Axum app
    let state_extractor = server_state.clone();
    let app = Router::new()
        .route("/", get(serve_index))
        .route("/ws", get(move |ws| handle_ws_route(ws, state_extractor.clone())));

    // 5. Setup TLS Acceptor
    let tls_config = generate_self_signed_config()?;
    let tls_acceptor = TlsAcceptor::from(Arc::new(tls_config));

    // 6. Bind listener
    let listener = TcpListener::bind(bind_addr).await?;
    println!("[Server] Listening on https://{}", bind_addr);

    // 7. Accept loop
    loop {
        let (tcp_stream, remote_addr) = match listener.accept().await {
            Ok(res) => res,
            Err(e) => {
                eprintln!("[Server] Failed to accept TCP connection: {:?}", e);
                continue;
            }
        };

        let tls_acceptor = tls_acceptor.clone();
        let app = app.clone();
        let state = server_state.clone();

        tokio::spawn(async move {
            let client_ip = remote_addr.ip().to_string();
            if is_ip_rate_limited(&state, &client_ip).await {
                return;
            }

            let tls_stream = match tls_acceptor.accept(tcp_stream).await {
                Ok(stream) => stream,
                Err(e) => {
                    eprintln!("[Server] TLS handshake failed with {}: {:?}", remote_addr, e);
                    return;
                }
            };

            let io = TokioIo::new(tls_stream);
            let hyper_service = hyper_util::service::TowerToHyperService::new(app);
            if let Err(err) = ConnBuilder::new(TokioExecutor::new())
                .serve_connection_with_upgrades(io, hyper_service)
                .await
            {
                let err_str = err.to_string();
                if !err_str.contains("connection closed") && !err_str.contains("Broken pipe") {
                    eprintln!("[Server] Connection error from {}: {:?}", remote_addr, err);
                }
            }
        });
    }
}

async fn serve_index() -> impl IntoResponse {
    Html(INDEX_HTML)
}

async fn handle_ws_route(ws: WebSocketUpgrade, state: Arc<ServerState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, state))
}

async fn handle_ws(socket: WebSocket, state: Arc<ServerState>) {
    let (ws_sender, mut ws_receiver) = socket.split();
    let ws_sender = Arc::new(Mutex::new(ws_sender));

    let mut authenticated = false;
    let mut pty_master: Option<Arc<Mutex<Box<dyn MasterPty + Send>>>> = None;
    let mut pty_writer: Option<Arc<Mutex<Box<dyn Write + Send>>>> = None;
    let mut client_session_name: Option<String> = None;

    // Read messages
    while let Some(msg_res) = ws_receiver.next().await {
        let msg = match msg_res {
            Ok(m) => m,
            Err(_) => break,
        };

        match msg {
            AxumMessage::Text(text) => {
                let cmd: ClientCommand = match serde_json::from_str(&text) {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                match cmd {
                    ClientCommand::Auth { password, session, cols, rows } => {
                        if password == state.password {
                            authenticated = true;
                            let response_json = serde_json::to_string(&ServerResponse::AuthOk).unwrap();
                            let _ = ws_sender.lock().await.send(AxumMessage::Text(response_json)).await;

                            if let Some(sess_name) = session {
                                match attach_client_session(
                                    &sess_name,
                                    cols.unwrap_or(80),
                                    rows.unwrap_or(24),
                                    ws_sender.clone(),
                                ).await {
                                    Ok((master, writer, client_sess)) => {
                                        pty_master = Some(Arc::new(Mutex::new(master)));
                                        pty_writer = Some(Arc::new(Mutex::new(writer)));
                                        client_session_name = Some(client_sess);
                                    }
                                    Err(e) => {
                                        eprintln!("[Server] Error attaching session: {:?}", e);
                                    }
                                }
                            }
                        } else {
                            let response_json = serde_json::to_string(&ServerResponse::AuthFail).unwrap();
                            let _ = ws_sender.lock().await.send(AxumMessage::Text(response_json)).await;
                            let _ = ws_sender.lock().await.close().await;
                            return;
                        }
                    }
                    ClientCommand::ListSessions => {
                        if !authenticated { return; }
                        let sessions = list_base_tmux_sessions();
                        let response_json = serde_json::to_string(&ServerResponse::SessionsList { sessions }).unwrap();
                        let _ = ws_sender.lock().await.send(AxumMessage::Text(response_json)).await;
                    }
                    ClientCommand::Attach { session, cols, rows } => {
                        if !authenticated { return; }
                        
                        // Clean up previous client session if any
                        if let Some(ref prev_sess) = client_session_name {
                            let _ = std::process::Command::new("tmux")
                                .args(&["kill-session", "-t", prev_sess])
                                .status();
                        }

                        match attach_client_session(
                            &session,
                            cols.unwrap_or(80),
                            rows.unwrap_or(24),
                            ws_sender.clone(),
                        ).await {
                            Ok((master, writer, client_sess)) => {
                                pty_master = Some(Arc::new(Mutex::new(master)));
                                pty_writer = Some(Arc::new(Mutex::new(writer)));
                                client_session_name = Some(client_sess);
                            }
                            Err(e) => {
                                eprintln!("[Server] Error attaching session: {:?}", e);
                            }
                        }
                    }
                    ClientCommand::Resize { cols, rows } => {
                        if !authenticated { return; }
                        if let Some(ref master) = pty_master {
                            let master_lock = master.lock().await;
                            let _ = master_lock.resize(PtySize {
                                rows,
                                cols,
                                pixel_width: 0,
                                pixel_height: 0,
                            });
                        }
                    }
                }
            }
            AxumMessage::Binary(data) => {
                if !authenticated { continue; }
                if let Some(ref writer) = pty_writer {
                    let mut writer_lock = writer.lock().await;
                    let _ = writer_lock.write_all(&data);
                    let _ = writer_lock.flush();
                }
            }
            _ => {}
        }
    }

    // Connection closed, clean up temporary client session
    if let Some(ref client_sess) = client_session_name {
        let _ = std::process::Command::new("tmux")
            .args(&["kill-session", "-t", client_sess])
            .status();
    }
}

async fn attach_client_session(
    base_name: &str,
    cols: u16,
    rows: u16,
    ws_sender: Arc<Mutex<futures_util::stream::SplitSink<WebSocket, AxumMessage>>>,
) -> Result<(Box<dyn MasterPty + Send>, Box<dyn Write + Send>, String), anyhow::Error> {
    // 1. Ensure the base session exists
    let has_session = std::process::Command::new("tmux")
        .args(&["has-session", "-t", base_name])
        .status();

    if has_session.is_err() || !has_session.unwrap().success() {
        // Create base session in the background
        let _ = std::process::Command::new("tmux")
            .args(&["new-session", "-d", "-s", base_name])
            .status();
    }

    // 2. Generate unique client session name grouped to the base session
    let random_id = rand::thread_rng().gen_range(1000..9999);
    let client_sess_name = format!("{}_client_{}", base_name, random_id);

    // 3. Create a PTY pair
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    // 4. Spawn: tmux new-session -t <base_name> -s <client_sess_name>
    let mut cmd = CommandBuilder::new("tmux");
    cmd.arg("new-session");
    cmd.arg("-A");
    cmd.arg("-t");
    cmd.arg(base_name);
    cmd.arg("-s");
    cmd.arg(&client_sess_name);

    let child = pair.slave.spawn_command(cmd);
    if child.is_err() {
        return Err(anyhow::anyhow!("Failed to spawn tmux command inside PTY"));
    }

    // Drop the slave in this process so master gets EOF when child exits
    drop(pair.slave);

    let pty_writer = pair.master.take_writer()?;

    // 5. Spawn OS thread to read from PTY master and write to this client WebSocket
    let mut reader = pair.master.try_clone_reader()?;
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 { break; }
            let data = buf[..n].to_vec();
            let sender = ws_sender.clone();
            
            // Write to websocket using blocking wrapper or running on mini-runtime
            let _ = rt.block_on(async {
                sender.lock().await.send(AxumMessage::Binary(data)).await
            });
        }
    });

    Ok((pair.master, pty_writer, client_sess_name))
}

fn list_base_tmux_sessions() -> Vec<String> {
    let output = std::process::Command::new("tmux")
        .args(&["list-sessions", "-F", "#{session_name}"])
        .output();

    let mut sessions = Vec::new();
    if let Ok(o) = output {
        if o.status.success() {
            let stdout = String::from_utf8_lossy(&o.stdout);
            for line in stdout.lines() {
                let name = line.trim();
                // Filter out temporary client connections
                if !name.is_empty() && !name.contains("_client_") {
                    sessions.push(name.to_string());
                }
            }
        }
    }
    sessions
}

fn get_config_dir() -> PathBuf {
    let mut path = dirs_next::home_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push(".config");
    path.push("tailmux");
    path
}

// Save the base sessions state (tmux layout) to disk
pub async fn save_sessions_state() -> Result<(), anyhow::Error> {
    let output = std::process::Command::new("tmux")
        .args(&["list-panes", "-a", "-F", "#{session_name} #{pane_current_path}"])
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return Ok(()),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut saved_sessions = Vec::new();
    let mut seen = HashMap::new();

    for line in stdout.lines() {
        let parts: Vec<&str> = line.splitn(2, ' ').collect();
        if parts.len() == 2 {
            let session_name = parts[0].to_string();
            let cwd = parts[1].to_string();
            
            // Skip temporary client sessions
            if session_name.contains("_client_") {
                continue;
            }

            if !seen.contains_key(&session_name) {
                seen.insert(session_name.clone(), true);
                saved_sessions.push(SavedSession { name: session_name, cwd });
            }
        }
    }

    if saved_sessions.is_empty() {
        return Ok(());
    }

    let config_dir = get_config_dir();
    std::fs::create_dir_all(&config_dir)?;
    let mut file_path = config_dir;
    file_path.push("sessions.json");

    let state = SavedState { sessions: saved_sessions };
    let json = serde_json::to_string_pretty(&state)?;
    std::fs::write(file_path, json)?;

    Ok(())
}

// Restore saved sessions on startup
pub async fn restore_sessions() -> Result<(), anyhow::Error> {
    let mut file_path = get_config_dir();
    file_path.push("sessions.json");

    if !file_path.exists() {
        return Ok(());
    }

    let data = std::fs::read_to_string(&file_path)?;
    let state: SavedState = serde_json::from_str(&data)?;

    println!("[Server] Restoring {} tmux sessions...", state.sessions.len());
    for sess in state.sessions {
        let _ = std::process::Command::new("tmux")
            .args(&["new-session", "-d", "-s", &sess.name, "-c", &sess.cwd])
            .status();
    }

    let _ = std::fs::remove_file(file_path);
    Ok(())
}

async fn is_ip_rate_limited(state: &Arc<ServerState>, ip: &str) -> bool {
    let mut history = state.connection_history.lock().await;
    let now = Instant::now();

    let tracker = history.entry(ip.to_string()).or_insert_with(|| ConnectionTracker {
        timestamps: Vec::new(),
        banned_until: None,
    });

    if let Some(banned_until) = tracker.banned_until {
        if now < banned_until {
            return true;
        } else {
            tracker.banned_until = None;
        }
    }

    tracker.timestamps.retain(|&t| now.duration_since(t) < Duration::from_secs(10));

    if tracker.timestamps.len() >= 3 {
        tracker.banned_until = Some(now + Duration::from_secs(300));
        println!("[Security] IP {} rate limited and banned for 5 minutes.", ip);
        return true;
    }

    tracker.timestamps.push(now);
    false
}
