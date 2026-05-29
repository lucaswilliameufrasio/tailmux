use std::env;
use std::net::SocketAddr;
use rand::Rng;

mod crypto;
mod web;
mod server;
mod client;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        return Ok(());
    }

    match args[1].as_str() {
        "server" => {
            let mut bind_addr: SocketAddr = "0.0.0.0:7788".parse().unwrap();
            let mut password = None;

            let mut i = 2;
            while i < args.len() {
                match args[i].as_str() {
                    "--bind" => {
                        if i + 1 < args.len() {
                            bind_addr = args[i + 1]
                                .parse()
                                .map_err(|_| anyhow::anyhow!("Invalid bind address (ex: 0.0.0.0:7788)"))?;
                            i += 2;
                        } else {
                            return Err(anyhow::anyhow!("Argument --bind requires a value"));
                        }
                    }
                    "--password" => {
                        if i + 1 < args.len() {
                            password = Some(args[i + 1].clone());
                            i += 2;
                        } else {
                            return Err(anyhow::anyhow!("Argument --password requires a value"));
                        }
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
                }
            }

            let password = match password {
                Some(p) => p,
                None => {
                    // Generate random 6 character alphanumeric code
                    let mut rng = rand::thread_rng();
                    let code: String = (0..6)
                        .map(|_| {
                            let idx = rng.gen_range(0..36);
                            if idx < 10 {
                                (b'0' + idx) as char
                            } else {
                                (b'A' + (idx - 10)) as char
                            }
                        })
                        .collect();
                    code
                }
            };

            println!("====================================================");
            println!("  TAILMUX SERVER STARTED");
            println!("  Address: {}", bind_addr);
            println!("  Access Password: {}", password);
            println!("====================================================");

            server::run_server(bind_addr, password).await?;
        }
        "client" => {
            let mut connect_addr: SocketAddr = "127.0.0.1:7788".parse().unwrap();
            let mut password = None;
            let mut session = None;

            let mut i = 2;
            while i < args.len() {
                match args[i].as_str() {
                    "--connect" => {
                        if i + 1 < args.len() {
                            connect_addr = args[i + 1]
                                .parse()
                                .map_err(|_| anyhow::anyhow!("Invalid connection address (ex: 127.0.0.1:7788)"))?;
                            i += 2;
                        } else {
                            return Err(anyhow::anyhow!("Argument --connect requires a value"));
                        }
                    }
                    "--password" => {
                        if i + 1 < args.len() {
                            password = Some(args[i + 1].clone());
                            i += 2;
                        } else {
                            return Err(anyhow::anyhow!("Argument --password requires a value"));
                        }
                    }
                    "--session" => {
                        if i + 1 < args.len() {
                            session = Some(args[i + 1].clone());
                            i += 2;
                        } else {
                            return Err(anyhow::anyhow!("Argument --session requires a value"));
                        }
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
                    }
                }
            }

            client::run_client(connect_addr, password, session).await?;
        }
        _ => {
            print_usage();
        }
    }

    Ok(())
}

fn print_usage() {
    println!("Tailmux - Secure Remote Tmux Wrapper over TLS/WebSockets");
    println!();
    println!("Usage:");
    println!("  tailmux server [options]   - Starts the server daemon");
    println!("  tailmux client [options]   - Connects to the server");
    println!();
    println!("Server Options:");
    println!("  --bind <ip:port>        Address to listen on (default: 0.0.0.0:7788)");
    println!("  --password <password>   Access password (if omitted, a random one will be generated)");
    println!();
    println!("Client Options:");
    println!("  --connect <ip:port>     Address of the server (default: 127.0.0.1:7788)");
    println!("  --password <password>   Access password (if omitted, will be securely prompted)");
    println!("  --session <name>         Name of the session to connect directly");
    println!();
}
