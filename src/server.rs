use axum::{
    extract::ws::{Message as AxumMessage, WebSocket, WebSocketUpgrade},
    extract::ConnectInfo,
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
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio_rustls::TlsAcceptor;

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

#[derive(Serialize)]
pub struct ActiveConnInfo {
    pub id: String,
    pub addr: String,
}

#[derive(Serialize)]
pub struct BannedIpInfo {
    pub ip: String,
    pub expires_in_secs: u64,
}

pub struct ServerState {
    pub password: Mutex<String>,
    pub connection_history: Mutex<HashMap<String, ConnectionTracker>>,
    pub active_connections: Mutex<HashMap<String, SocketAddr>>,
}

#[derive(Deserialize)]
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
    #[serde(rename = "admin_get_status")]
    AdminGetStatus,
    #[serde(rename = "admin_change_password")]
    AdminChangePassword { new_password: String },
    #[serde(rename = "admin_unban_ip")]
    AdminUnbanIp { ip: String },
}

#[derive(Serialize)]
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
    #[serde(rename = "admin_status")]
    AdminStatus {
        connections: Vec<ActiveConnInfo>,
        banned_ips: Vec<BannedIpInfo>,
        current_password: String,
    },
    #[serde(rename = "admin_password_changed")]
    AdminPasswordChanged,
}

// Tower middleware to inject SocketAddr as ConnectInfo extension for Axum
#[derive(Clone)]
struct AddConnectInfoService<S> {
    inner: S,
    remote_addr: SocketAddr,
}

impl<S, ReqBody, ResBody> tower_service::Service<hyper::Request<ReqBody>>
    for AddConnectInfoService<S>
where
    S: tower_service::Service<hyper::Request<ReqBody>, Response = hyper::Response<ResBody>> + Clone,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: hyper::Request<ReqBody>) -> Self::Future {
        req.extensions_mut().insert(ConnectInfo(self.remote_addr));
        self.inner.call(req)
    }
}

pub async fn run_server(bind_addr: SocketAddr, password: String) -> Result<(), anyhow::Error> {
    // 1. Restore previous tmux sessions if they were saved before reboot
    if let Err(e) = restore_sessions().await {
        eprintln!("[Server] Error restoring tmux sessions: {:?}", e);
    }

    // 2. Setup server state
    let server_state = Arc::new(ServerState {
        password: Mutex::new(password),
        connection_history: Mutex::new(HashMap::new()),
        active_connections: Mutex::new(HashMap::new()),
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
    let app = Router::new().route("/", get(serve_index)).route(
        "/ws",
        get(move |ws, info| handle_ws_route(ws, info, state_extractor.clone())),
    );

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
            if is_ip_banned(&state, &client_ip).await {
                return;
            }

            let tls_stream = match tls_acceptor.accept(tcp_stream).await {
                Ok(stream) => stream,
                Err(e) => {
                    eprintln!(
                        "[Server] TLS handshake failed with {}: {:?}",
                        remote_addr, e
                    );
                    return;
                }
            };

            let io = TokioIo::new(tls_stream);
            // Wrap the axum app router service to inject ConnectInfo(remote_addr)
            let wrapped_service = AddConnectInfoService {
                inner: app.clone(),
                remote_addr,
            };
            let hyper_service = hyper_util::service::TowerToHyperService::new(wrapped_service);
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

async fn handle_ws_route(
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    state: Arc<ServerState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, addr, state))
}

async fn handle_ws(socket: WebSocket, client_addr: SocketAddr, state: Arc<ServerState>) {
    let (ws_sender, mut ws_receiver) = socket.split();
    let ws_sender = Arc::new(Mutex::new(ws_sender));

    let mut authenticated = false;
    let mut pty_master: Option<Arc<Mutex<Box<dyn MasterPty + Send>>>> = None;
    let mut pty_writer: Option<Arc<Mutex<Box<dyn Write + Send>>>> = None;
    let mut client_session_name: Option<String> = None;

    // Generate a unique connection ID for the admin panel
    let connection_id = format!("{:x}", rand::thread_rng().gen::<u64>());

    // Generate server's ephemeral DH keypair and random salt challenge
    let (server_priv, server_pub_bytes) = match crate::crypto::generate_ecdh_keypair() {
        Ok(val) => val,
        Err(e) => {
            eprintln!("[Server] Failed to generate DH keypair: {:?}", e);
            let _ = ws_sender.lock().await.close().await;
            return;
        }
    };

    let salt: String = (0..16)
        .map(|_| format!("{:02x}", rand::thread_rng().gen::<u8>()))
        .collect();

    let server_pub_hex: String = server_pub_bytes
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect();

    // Send challenge immediately
    let challenge_msg = ServerResponse::AuthChallenge {
        salt: salt.clone(),
        public_key: server_pub_hex,
    };

    let challenge_json = serde_json::to_string(&challenge_msg).unwrap();
    if ws_sender
        .lock()
        .await
        .send(AxumMessage::Text(challenge_json.into()))
        .await
        .is_err()
    {
        return;
    }

    // Keep server_priv in an Option so we can consume it once upon AuthResponse
    let mut server_priv_opt = Some(server_priv);

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
                    ClientCommand::AuthResponse {
                        public_key,
                        proof,
                        session,
                        cols,
                        rows,
                    } => {
                        let client_pub_bytes = match hex_to_bytes(&public_key) {
                            Ok(bytes) => bytes,
                            Err(_) => {
                                let _ = ws_sender
                                    .lock()
                                    .await
                                    .send(AxumMessage::Text(
                                        serde_json::to_string(&ServerResponse::AuthFail)
                                            .unwrap()
                                            .into(),
                                    ))
                                    .await;
                                return;
                            }
                        };

                        let server_priv = match server_priv_opt.take() {
                            Some(priv_key) => priv_key,
                            None => {
                                // Already consumed or invalid
                                let _ = ws_sender
                                    .lock()
                                    .await
                                    .send(AxumMessage::Text(
                                        serde_json::to_string(&ServerResponse::AuthFail)
                                            .unwrap()
                                            .into(),
                                    ))
                                    .await;
                                return;
                            }
                        };

                        let shared_secret = match crate::crypto::derive_shared_secret(
                            server_priv,
                            &client_pub_bytes,
                        ) {
                            Ok(secret) => secret,
                            Err(e) => {
                                eprintln!("[Server] DH shared secret derivation failed: {:?}", e);
                                let _ = ws_sender
                                    .lock()
                                    .await
                                    .send(AxumMessage::Text(
                                        serde_json::to_string(&ServerResponse::AuthFail)
                                            .unwrap()
                                            .into(),
                                    ))
                                    .await;
                                return;
                            }
                        };

                        let client_ip = client_addr.ip().to_string();
                        let pass_matches = {
                            let pass_lock = state.password.lock().await;
                            let expected_proof = crate::crypto::compute_auth_proof(
                                &pass_lock,
                                &shared_secret,
                                &salt,
                            );
                            proof == expected_proof
                        };

                        if pass_matches {
                            authenticated = true;
                            record_successful_auth(&state, &client_ip).await;

                            // Insert into active connections
                            state
                                .active_connections
                                .lock()
                                .await
                                .insert(connection_id.clone(), client_addr);

                            let response_json =
                                serde_json::to_string(&ServerResponse::AuthOk).unwrap();
                            let _ = ws_sender
                                .lock()
                                .await
                                .send(AxumMessage::Text(response_json.into()))
                                .await;

                            if let Some(sess_name) = session {
                                match attach_client_session(
                                    &sess_name,
                                    cols.unwrap_or(80),
                                    rows.unwrap_or(24),
                                    ws_sender.clone(),
                                )
                                .await
                                {
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
                            record_failed_auth(&state, &client_ip).await;
                            let response_json =
                                serde_json::to_string(&ServerResponse::AuthFail).unwrap();
                            let _ = ws_sender
                                .lock()
                                .await
                                .send(AxumMessage::Text(response_json.into()))
                                .await;
                            let _ = ws_sender.lock().await.close().await;
                            return;
                        }
                    }
                    ClientCommand::ListSessions => {
                        if !authenticated {
                            return;
                        }
                        let sessions = list_base_tmux_sessions();
                        let response_json =
                            serde_json::to_string(&ServerResponse::SessionsList { sessions })
                                .unwrap();
                        let _ = ws_sender
                            .lock()
                            .await
                            .send(AxumMessage::Text(response_json.into()))
                            .await;
                    }
                    ClientCommand::Attach {
                        session,
                        cols,
                        rows,
                    } => {
                        if !authenticated {
                            return;
                        }

                        // Clean up previous client session if any
                        if let Some(ref prev_sess) = client_session_name {
                            let _ = std::process::Command::new("tmux")
                                .args(["kill-session", "-t", prev_sess])
                                .status();
                        }

                        match attach_client_session(
                            &session,
                            cols.unwrap_or(80),
                            rows.unwrap_or(24),
                            ws_sender.clone(),
                        )
                        .await
                        {
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
                        if !authenticated {
                            return;
                        }
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
                    ClientCommand::AdminGetStatus => {
                        if !authenticated {
                            return;
                        }
                        let status_resp = get_admin_status_response(&state).await;
                        let _ = ws_sender
                            .lock()
                            .await
                            .send(AxumMessage::Text(
                                serde_json::to_string(&status_resp).unwrap().into(),
                            ))
                            .await;
                    }
                    ClientCommand::AdminChangePassword { new_password } => {
                        if !authenticated {
                            return;
                        }
                        {
                            let mut pass_lock = state.password.lock().await;
                            *pass_lock = new_password.clone();
                        }
                        if let Err(e) = save_persisted_password(&new_password) {
                            eprintln!(
                                "[Server] Failed to save updated password to config: {:?}",
                                e
                            );
                        }
                        println!("[Security] Access password updated via admin panel.");
                        let _ = ws_sender
                            .lock()
                            .await
                            .send(AxumMessage::Text(
                                serde_json::to_string(&ServerResponse::AdminPasswordChanged)
                                    .unwrap()
                                    .into(),
                            ))
                            .await;

                        let status_resp = get_admin_status_response(&state).await;
                        let _ = ws_sender
                            .lock()
                            .await
                            .send(AxumMessage::Text(
                                serde_json::to_string(&status_resp).unwrap().into(),
                            ))
                            .await;
                    }
                    ClientCommand::AdminUnbanIp { ip } => {
                        if !authenticated {
                            return;
                        }
                        let mut history = state.connection_history.lock().await;
                        if let Some(tracker) = history.get_mut(&ip) {
                            tracker.banned_until = None;
                            tracker.timestamps.clear();
                            println!("[Security] IP {} manually unbanned via admin panel.", ip);
                        }

                        let status_resp = get_admin_status_response(&state).await;
                        let _ = ws_sender
                            .lock()
                            .await
                            .send(AxumMessage::Text(
                                serde_json::to_string(&status_resp).unwrap().into(),
                            ))
                            .await;
                    }
                }
            }
            AxumMessage::Binary(data) => {
                if !authenticated {
                    continue;
                }
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
            .args(["kill-session", "-t", client_sess])
            .status();
    }

    // Remove from active connections list
    state.active_connections.lock().await.remove(&connection_id);
}

async fn get_admin_status_response(state: &Arc<ServerState>) -> ServerResponse {
    let conns_map = state.active_connections.lock().await;
    let mut connections = Vec::new();
    for (id, addr) in conns_map.iter() {
        connections.push(ActiveConnInfo {
            id: id.clone(),
            addr: addr.to_string(),
        });
    }

    let history_map = state.connection_history.lock().await;
    let mut banned_ips = Vec::new();
    let now = Instant::now();
    for (ip, tracker) in history_map.iter() {
        if let Some(banned_until) = tracker.banned_until {
            if now < banned_until {
                let expires_in_secs = banned_until.duration_since(now).as_secs();
                banned_ips.push(BannedIpInfo {
                    ip: ip.clone(),
                    expires_in_secs,
                });
            }
        }
    }

    let current_password = {
        let pass_lock = state.password.lock().await;
        pass_lock.clone()
    };

    ServerResponse::AdminStatus {
        connections,
        banned_ips,
        current_password,
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
        .args(["has-session", "-t", base_name])
        .status();

    if has_session.is_err() || !has_session.unwrap().success() {
        // Create base session in the background
        let _ = std::process::Command::new("tmux")
            .args(["new-session", "-d", "-s", base_name])
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
            if n == 0 {
                break;
            }
            let data = buf[..n].to_vec();
            let sender = ws_sender.clone();

            let _ = rt.block_on(async {
                sender
                    .lock()
                    .await
                    .send(AxumMessage::Binary(data.into()))
                    .await
            });
        }
    });

    Ok((pair.master, pty_writer, client_sess_name))
}

fn list_base_tmux_sessions() -> Vec<String> {
    let output = std::process::Command::new("tmux")
        .args(["list-sessions", "-F", "#{session_name}"])
        .output();

    let mut sessions = Vec::new();
    if let Ok(o) = output {
        if o.status.success() {
            let stdout = String::from_utf8_lossy(&o.stdout);
            for line in stdout.lines() {
                let name = line.trim();
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
pub async fn save_sessions_state() -> Result<Vec<String>, anyhow::Error> {
    let output = std::process::Command::new("tmux")
        .args([
            "list-panes",
            "-a",
            "-F",
            "#{session_name} #{pane_current_path}",
        ])
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return Ok(Vec::new()),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut saved_sessions = Vec::new();
    let mut seen = HashMap::new();

    for line in stdout.lines() {
        let parts: Vec<&str> = line.splitn(2, ' ').collect();
        if parts.len() == 2 {
            let session_name = parts[0].to_string();
            let cwd = parts[1].to_string();

            if session_name.contains("_client_") {
                continue;
            }

            if !seen.contains_key(&session_name) {
                seen.insert(session_name.clone(), true);
                saved_sessions.push(SavedSession {
                    name: session_name,
                    cwd,
                });
            }
        }
    }

    if saved_sessions.is_empty() {
        return Ok(Vec::new());
    }

    let config_dir = get_config_dir();
    std::fs::create_dir_all(&config_dir)?;
    let mut file_path = config_dir;
    file_path.push("sessions.json");

    let saved_names: Vec<String> = saved_sessions.iter().map(|s| s.name.clone()).collect();

    let state = SavedState {
        sessions: saved_sessions,
    };
    let json = serde_json::to_string_pretty(&state)?;
    std::fs::write(file_path, json)?;

    Ok(saved_names)
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

    println!(
        "[Server] Restoring {} tmux sessions...",
        state.sessions.len()
    );
    for sess in state.sessions {
        let _ = std::process::Command::new("tmux")
            .args(["new-session", "-d", "-s", &sess.name, "-c", &sess.cwd])
            .status();
    }

    let _ = std::fs::remove_file(file_path);
    Ok(())
}

async fn is_ip_banned(state: &Arc<ServerState>, ip: &str) -> bool {
    let mut history = state.connection_history.lock().await;
    let now = Instant::now();

    let tracker = history
        .entry(ip.to_string())
        .or_insert_with(|| ConnectionTracker {
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
    false
}

async fn record_failed_auth(state: &Arc<ServerState>, ip: &str) {
    let mut history = state.connection_history.lock().await;
    let now = Instant::now();

    let tracker = history
        .entry(ip.to_string())
        .or_insert_with(|| ConnectionTracker {
            timestamps: Vec::new(),
            banned_until: None,
        });

    tracker
        .timestamps
        .retain(|&t| now.duration_since(t) < Duration::from_secs(30));
    tracker.timestamps.push(now);

    if tracker.timestamps.len() >= 5 {
        tracker.banned_until = Some(now + Duration::from_secs(300));
        println!(
            "[Security] IP {} banned for 5 minutes due to 5 authentication failures in 30 seconds.",
            ip
        );
    }
}

async fn record_successful_auth(state: &Arc<ServerState>, ip: &str) {
    let mut history = state.connection_history.lock().await;
    if let Some(tracker) = history.get_mut(ip) {
        tracker.timestamps.clear();
        tracker.banned_until = None;
    }
}

fn get_password_file_path() -> PathBuf {
    let mut path = get_config_dir();
    path.push("password.txt");
    path
}

pub fn load_or_generate_password(provided_password: Option<String>) -> String {
    let path = get_password_file_path();
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            let pwd = content.trim().to_string();
            if !pwd.is_empty() {
                println!(
                    "[Server] Loaded persisted access password from ~/.config/tailmux/password.txt"
                );
                return pwd;
            }
        }
    }

    // Otherwise, initialize it
    let pwd = match provided_password {
        Some(p) => p,
        None => {
            // Generate random 6 character alphanumeric code
            let mut rng = rand::thread_rng();
            (0..6)
                .map(|_| {
                    let idx = rng.gen_range(0..36);
                    if idx < 10 {
                        (b'0' + idx) as char
                    } else {
                        (b'A' + (idx - 10)) as char
                    }
                })
                .collect()
        }
    };

    if let Err(e) = save_persisted_password(&pwd) {
        eprintln!("[Server] Error saving password: {:?}", e);
    }
    pwd
}

pub fn save_persisted_password(password: &str) -> Result<(), anyhow::Error> {
    let config_dir = get_config_dir();
    std::fs::create_dir_all(&config_dir)?;
    let path = get_password_file_path();
    std::fs::write(&path, password)?;
    Ok(())
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

pub fn require_tmux() {
    if !check_tmux_installed() {
        let os = std::env::consts::OS;
        eprintln!();
        eprintln!("Error: tmux is not installed or not found in PATH.");
        eprintln!();
        eprintln!("Tailmux requires tmux to manage terminal sessions.");
        eprintln!("Install it using your system's package manager:");
        eprintln!();
        print!("{}", tmux_install_instructions(os));
        eprintln!("After installing tmux, run tailmux again.");
        std::process::exit(1);
    }
}

fn check_tmux_installed() -> bool {
    std::process::Command::new("tmux")
        .arg("-V")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn tmux_install_instructions(os: &str) -> String {
    match os {
        "linux" => {
            "\
  Debian/Ubuntu:  sudo apt install tmux
  Fedora/RHEL:    sudo dnf install tmux
  Arch Linux:     sudo pacman -S tmux
  Alpine:         apk add tmux
  openSUSE:       sudo zypper install tmux
"
        }
        "macos" => {
            "\
  Homebrew:  brew install tmux
  MacPorts:  sudo port install tmux
"
        }
        "windows" => {
            "\
  WSL (Debian/Ubuntu):  sudo apt install tmux
  Scoop:               scoop install tmux
  Chocolatey:          choco install tmux
"
        }
        _ => "  Please install tmux using your system's package manager.\n",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tmux_install_instructions_linux() {
        let msg = tmux_install_instructions("linux");
        assert!(msg.contains("apt install tmux"));
        assert!(msg.contains("dnf install tmux"));
        assert!(msg.contains("pacman -S tmux"));
        assert!(msg.contains("apk add tmux"));
        assert!(msg.contains("zypper install tmux"));
    }

    #[test]
    fn test_tmux_install_instructions_macos() {
        let msg = tmux_install_instructions("macos");
        assert!(msg.contains("brew install tmux"));
        assert!(msg.contains("port install tmux"));
    }

    #[test]
    fn test_tmux_install_instructions_windows() {
        let msg = tmux_install_instructions("windows");
        assert!(msg.contains("scoop install tmux"));
        assert!(msg.contains("choco install tmux"));
        assert!(msg.contains("apt install tmux"));
    }

    #[test]
    fn test_tmux_install_instructions_unknown() {
        let msg = tmux_install_instructions("freebsd");
        assert!(msg.contains("package manager"));
    }

    #[test]
    fn test_tmux_install_instructions_non_empty() {
        for os in &["linux", "macos", "windows", "freebsd", ""] {
            let msg = tmux_install_instructions(os);
            assert!(!msg.is_empty(), "Empty instructions for OS: {}", os);
        }
    }

    #[test]
    fn test_hex_to_bytes() {
        assert_eq!(
            hex_to_bytes("001122aabbff").unwrap(),
            vec![0x00, 0x11, 0x22, 0xaa, 0xbb, 0xff]
        );
        assert_eq!(hex_to_bytes("").unwrap(), Vec::<u8>::new());
        assert!(hex_to_bytes("g").is_err());
        assert!(hex_to_bytes("0").is_err()); // odd length
    }

    #[tokio::test]
    async fn test_rate_limiting_ip_banning() {
        let state = Arc::new(ServerState {
            password: Mutex::new("test".to_string()),
            connection_history: Mutex::new(HashMap::new()),
            active_connections: Mutex::new(HashMap::new()),
        });

        let test_ip = "127.0.0.99";

        // Initially not banned
        assert!(!is_ip_banned(&state, test_ip).await);

        // Register 4 failures -> still not banned
        for _ in 0..4 {
            record_failed_auth(&state, test_ip).await;
            assert!(!is_ip_banned(&state, test_ip).await);
        }

        // Register 5th failure -> now banned
        record_failed_auth(&state, test_ip).await;
        assert!(is_ip_banned(&state, test_ip).await);

        // Record successful auth on banned IP should unban it
        record_successful_auth(&state, test_ip).await;
        assert!(!is_ip_banned(&state, test_ip).await);
    }
}
