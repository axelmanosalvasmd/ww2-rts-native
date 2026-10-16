use std::{env, net::IpAddr};
use tokio::net::TcpListener;
use ww2_server::shards::{serve_sharded, shard_count};
use ww2_server::{Config, MapEditor, TrustedRoomConfig, serve, serve_trusted_room};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut explicit_bind = None;
    let mut trusted_room = None;
    let mut seed = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--bind" => explicit_bind = Some(args.next().ok_or("--bind requires HOST:PORT")?),
            "--trusted-room" if trusted_room.is_none() => {
                trusted_room = Some(args.next().ok_or("--trusted-room requires CODE")?);
            }
            "--seed" if seed.is_none() => seed = Some(args.next().ok_or("--seed requires U64")?),
            "--help" | "-h" => {
                println!(
                    "ww2-server [--bind HOST:PORT] [--trusted-room CODE --seed U64]\nAddress priority: --bind, WW2_BIND, HOST/PORT, 127.0.0.1:3000. Port 0 selects a free port.\nWW2_SHARDS: 1-64 isolated game threads (default 1).\nTrusted mode: one private two-human Conquest room, named armies, sandbox off, logistics off (temporary). Seeds/receipts stay host-private."
                );
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
    }
    let trusted = match (trusted_room, seed) {
        (None, None) => None,
        (Some(code), Some(seed)) => Some(TrustedRoomConfig::new(code, seed.parse::<u64>()?)?),
        _ => return Err("--trusted-room and --seed must be supplied together".into()),
    };
    let address = bind_address(
        env::var("HOST").ok().as_deref(),
        env::var("PORT").ok().as_deref(),
        env::var("WW2_BIND").ok().as_deref(),
        explicit_bind.as_deref(),
    )?;
    let listener = TcpListener::bind(&address).await?;
    println!("LISTENING http://{}", listener.local_addr()?);
    let config = Config {
        map_editor: Some(MapEditor::from_password_file(
            "maps".into(),
            env::var("EDIT_PASSWORD").ok().as_deref(),
            std::path::Path::new(".edit-password"),
        )?),
        saves_dir: env::var_os("SAVES_DIR")
            .map(Into::into)
            .unwrap_or_else(|| "saves".into()),
        public_url: env::var("PUBLIC_URL").unwrap_or_default(),
        diagnostic_dir: (env::var("WW2_DIAG").as_deref() == Ok("1")).then(|| "logs".into()),
        tick_log: env::var("WW2_TICKLOG").as_deref() == Ok("1"),
        ..Config::default()
    };
    let shutdown = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    let shards = shard_count(env::var("WW2_SHARDS").ok().as_deref());
    if shards > 1 {
        serve_sharded(listener, config, shutdown, trusted, shards).await?;
    } else if let Some(trusted) = trusted {
        serve_trusted_room(listener, config, shutdown, trusted).await?;
    } else {
        serve(listener, config, shutdown).await?;
    }
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

#[cfg(test)]
#[path = "net_bind_tests.rs"]
mod net_batch50_bind;
