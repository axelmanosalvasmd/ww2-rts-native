use std::{env, net::SocketAddr};
use tokio::net::TcpListener;
use ww2_server::{Config, serve};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut bind = env::var("WW2_BIND").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--bind" => bind = args.next().ok_or("--bind requires IP:PORT")?,
            "--help" | "-h" => {
                println!(
                    "ww2-server [--bind IP:PORT]\nDefault: WW2_BIND or 127.0.0.1:8080. Port 0 selects a free port."
                );
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
    }
    let address: SocketAddr = bind.parse()?;
    let listener = TcpListener::bind(address).await?;
    println!("LISTENING http://{}", listener.local_addr()?);
    let config = Config {
        public_url: env::var("PUBLIC_URL").unwrap_or_default(),
        ..Config::default()
    };
    serve(listener, config, async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await?;
    Ok(())
}
