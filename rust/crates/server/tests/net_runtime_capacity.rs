use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpStream},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::Duration,
};

struct BinaryServer {
    child: Child,
    address: SocketAddr,
}

impl BinaryServer {
    fn start(max_rooms: Option<&str>) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_ww2-server"));
        command
            .args(["--bind", "127.0.0.1:0"])
            .env_remove("MAX_ROOMS")
            .env_remove("PUBLIC_URL")
            .env_remove("WW2_BIND")
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        if let Some(value) = max_rooms {
            command.env("MAX_ROOMS", value);
        }
        let mut child = command.spawn().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let mut line = String::new();
            let result = BufReader::new(stdout).read_line(&mut line).map(|_| line);
            let _ = sender.send(result);
        });
        let mut server = Self {
            child,
            address: "127.0.0.1:0".parse().unwrap(),
        };
        let line = receiver
            .recv_timeout(Duration::from_secs(20))
            .expect("binary listening deadline")
            .unwrap();
        server.address = line
            .trim()
            .strip_prefix("LISTENING http://")
            .expect("binary listening address")
            .parse()
            .unwrap();
        server
    }
}

impl Drop for BinaryServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Client(TcpStream);

impl Client {
    fn open(address: SocketAddr, room: &str, token: &str) -> Self {
        let mut socket = TcpStream::connect(address).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        write!(socket,"GET /ws?room={room} HTTP/1.1\r\nHost: {address}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n").unwrap();
        let mut headers = Vec::new();
        while !headers.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            socket.read_exact(&mut byte).unwrap();
            headers.push(byte[0]);
            assert!(headers.len() < 8192);
        }
        assert!(
            String::from_utf8(headers)
                .unwrap()
                .starts_with("HTTP/1.1 101")
        );
        let mut client = Self(socket);
        client.send(json!({"t":"hello","name":token,"token":token,"proto":0}));
        client
    }

    fn send(&mut self, value: Value) {
        let body = serde_json::to_vec(&value).unwrap();
        let mask = [13_u8, 17, 19, 23];
        let mut frame = vec![0x81];
        if body.len() < 126 {
            frame.push(0x80 | body.len() as u8);
        } else {
            frame.push(0x80 | 126);
            frame.extend(u16::try_from(body.len()).unwrap().to_be_bytes());
        }
        frame.extend(mask);
        frame.extend(body.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
        self.0.write_all(&frame).unwrap();
    }

    fn frame(&mut self) -> (u8, Vec<u8>) {
        let mut header = [0; 2];
        self.0.read_exact(&mut header).unwrap();
        assert_eq!(header[1] & 0x80, 0);
        assert_ne!(header[0] & 0x80, 0);
        let size = match header[1] & 127 {
            126 => {
                let mut bytes = [0; 2];
                self.0.read_exact(&mut bytes).unwrap();
                usize::from(u16::from_be_bytes(bytes))
            }
            127 => {
                let mut bytes = [0; 8];
                self.0.read_exact(&mut bytes).unwrap();
                usize::try_from(u64::from_be_bytes(bytes)).unwrap()
            }
            size => usize::from(size),
        };
        assert!(size < 1024 * 1024);
        let mut body = vec![0; size];
        self.0.read_exact(&mut body).unwrap();
        (header[0] & 15, body)
    }

    fn receive(&mut self) -> Value {
        let (opcode, body) = self.frame();
        assert_eq!(opcode, 1);
        serde_json::from_slice(&body).unwrap()
    }
}

#[test]
fn intent_net16_binary_room_cap_rejects_new_rooms_and_keeps_existing_rooms_reachable() {
    for (setting, capacity) in [
        (None, 32),
        (Some("2"), 2),
        (Some("-7"), 1),
        (Some("999"), 256),
    ] {
        let server = BinaryServer::start(setting);
        let mut occupants = Vec::new();
        for index in 0..capacity {
            let code = format!("Capacity{index}");
            let mut client = Client::open(server.address, &code, "host");
            let lobby = client.receive();
            assert_eq!(lobby["t"], "lobby", "MAX_ROOMS={setting:?}, room {index}");
            assert_eq!(lobby["code"], code);
            occupants.push(client);
        }
        let mut overflow = Client::open(server.address, "Overflow", "overflow");
        assert_eq!(overflow.receive(), json!({"t":"full","reason":"capacity"}));
        let (opcode, body) = overflow.frame();
        assert_eq!(opcode, 8, "capacity rejection must close the socket");
        assert_eq!(body, 1000_u16.to_be_bytes());

        for index in [0, capacity - 1] {
            let code = format!("Capacity{index}");
            let mut guest = Client::open(server.address, &code, "guest");
            let lobby = guest.receive();
            assert_eq!(lobby["t"], "lobby");
            assert_eq!(lobby["code"], code);
            assert!(
                lobby["players"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|player| { player["name"] == "guest" && player["connected"] == true })
            );
        }
        for client in &mut occupants {
            client.send(json!({"t":"ping","c":"still-reachable"}));
            loop {
                let message = client.receive();
                if message["t"] == "pong" {
                    assert_eq!(message["c"], "still-reachable");
                    break;
                }
                assert_eq!(message["t"], "lobby");
            }
        }
    }
}
