use super::*;

#[tokio::test]
async fn net_batch50_bind_defaults_overrides_and_port_zero_are_reachable() {
    assert_eq!(
        bind_address(None, None, None, None).unwrap(),
        "127.0.0.1:3000"
    );
    assert_eq!(
        bind_address(Some("127.0.0.2"), Some("3456"), None, None).unwrap(),
        "127.0.0.2:3456"
    );
    for configured in [
        bind_address(Some("127.0.0.1"), Some("0"), None, None),
        bind_address(Some("invalid"), Some("invalid"), Some("127.0.0.1:0"), None),
        bind_address(
            Some("invalid"),
            Some("invalid"),
            Some("invalid"),
            Some("127.0.0.1:0"),
        ),
    ] {
        let listener = TcpListener::bind(configured.unwrap()).await.unwrap();
        let assigned = listener.local_addr().unwrap();
        assert_ne!(assigned.port(), 0);
        let connected = tokio::net::TcpStream::connect(assigned).await.unwrap();
        let (accepted, remote) = listener.accept().await.unwrap();
        assert_eq!(connected.local_addr().unwrap(), remote);
        assert_eq!(accepted.local_addr().unwrap(), assigned);
    }
}
