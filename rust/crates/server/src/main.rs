use std::{env, net::IpAddr};
use tokio::net::TcpListener;
use ww2_server::{Config, serve};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut explicit_bind = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--bind" => explicit_bind = Some(args.next().ok_or("--bind requires HOST:PORT")?),
            "--help" | "-h" => {
                println!(
                    "ww2-server [--bind HOST:PORT]\nAddress priority: --bind, WW2_BIND, HOST/PORT, 127.0.0.1:3000. Port 0 selects a free port."
                );
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
    }
    let address = bind_address(
        env::var("HOST").ok().as_deref(),
        env::var("PORT").ok().as_deref(),
        env::var("WW2_BIND").ok().as_deref(),
        explicit_bind.as_deref(),
    )?;
    let listener = TcpListener::bind(&address).await?;
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

fn bind_address(
    host: Option<&str>,
    port: Option<&str>,
    alias: Option<&str>,
    explicit: Option<&str>,
) -> Result<String, String> {
    let address = if let Some(bind) = explicit.or(alias) {
        bind.to_owned()
    } else {
        let host = host.filter(|s| !s.is_empty()).unwrap_or("127.0.0.1");
        let port = port.filter(|s| !s.is_empty()).unwrap_or("3000");
        if matches!(host.parse::<IpAddr>(), Ok(IpAddr::V6(_))) {
            format!("[{host}]:{port}")
        } else {
            format!("{host}:{port}")
        }
    };
    let (host, port) = address
        .rsplit_once(':')
        .ok_or("bind address requires HOST:PORT")?;
    if host.is_empty() {
        return Err("bind address requires a host".into());
    }
    port.parse::<u16>()
        .map_err(|_| "port must be an integer from 0 to 65535")?;
    Ok(address)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bind_precedence_defaults_and_validation_are_environment_independent() {
        assert_eq!(
            bind_address(None, None, None, None).unwrap(),
            "127.0.0.1:3000"
        );
        assert_eq!(
            bind_address(Some("localhost"), Some("0"), None, None).unwrap(),
            "localhost:0"
        );
        assert_eq!(
            bind_address(Some("::1"), Some("3100"), None, None).unwrap(),
            "[::1]:3100"
        );
        assert_eq!(
            bind_address(Some("bad"), Some("bad"), Some("127.0.0.1:3200"), None).unwrap(),
            "127.0.0.1:3200"
        );
        assert_eq!(
            bind_address(Some("bad"), Some("bad"), Some("bad"), Some("127.0.0.1:0")).unwrap(),
            "127.0.0.1:0"
        );
        for port in ["bad", "-1", "65536", "1.5"] {
            assert!(bind_address(None, Some(port), None, None).is_err());
        }
        assert!(bind_address(None, None, Some("127.0.0.1:no"), None).is_err());
        assert!(bind_address(None, None, None, Some(":3000")).is_err());
    }
}
