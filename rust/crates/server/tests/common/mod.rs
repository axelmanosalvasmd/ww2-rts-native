use serde_json::{Value, json};
use std::{
    fs::{self, File},
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpStream},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};
use ww2_core::client_view::ClientView;

static NEXT: AtomicU64 = AtomicU64::new(1);

// The child may expose only part of a JSON record, including a UTF-8 tail.
fn complete_records<'a>(
    bytes: &'a [u8],
    prefix: &'a [u8],
) -> impl Iterator<Item = serde_json::Result<Value>> + 'a {
    bytes
        .split_inclusive(|&byte| byte == b'\n')
        .filter(|line| line.ends_with(b"\n"))
        .filter_map(move |line| line.strip_prefix(prefix))
        .map(serde_json::from_slice)
}

#[test]
fn private_receipt_records_wait_for_complete_lines() {
    for prefix in [b"NATIVE_MATCH ".as_slice(), b"NATIVE_FAULT ".as_slice()] {
        let first = json!({"name":"é", "record":0});
        let next = json!({"name":"雪", "record":1});
        let line = |value: &Value| {
            let mut bytes = prefix.to_vec();
            bytes.extend(serde_json::to_vec(value).unwrap());
            bytes.push(b'\n');
            bytes
        };
        let prior = line(&first);
        let pending = line(&next);
        let mut split_utf8 = false;
        for split in 0..=pending.len() {
            split_utf8 |= std::str::from_utf8(&pending[..split]).is_err();
            let mut bytes = prior.clone();
            bytes.extend_from_slice(&pending[..split]);
            let expected = if split == pending.len() {
                vec![first.clone(), next.clone()]
            } else {
                vec![first.clone()]
            };
            assert_eq!(
                complete_records(&bytes, prefix)
                    .collect::<Result<Vec<_>, _>>()
                    .unwrap(),
                expected,
                "split {split}"
            );
            assert_eq!(
                complete_records(&pending[..split], prefix)
                    .collect::<Result<Vec<_>, _>>()
                    .unwrap(),
                if split == pending.len() {
                    vec![next.clone()]
                } else {
                    vec![]
                },
                "standalone split {split}"
            );
            bytes.extend_from_slice(&pending[split..]);
            assert_eq!(
                complete_records(&bytes, prefix)
                    .collect::<Result<Vec<_>, _>>()
                    .unwrap(),
                vec![first.clone(), next.clone()]
            );
        }
        assert!(
            split_utf8,
            "the split points must include a partial UTF-8 character"
        );
        for invalid in [b"{bad}\n".as_slice(), b"{\"name\":\"\xff\"}\n".as_slice()] {
            let mut malformed = prefix.to_vec();
            malformed.extend_from_slice(invalid);
            assert!(
                complete_records(&malformed, prefix)
                    .next()
                    .unwrap()
                    .is_err()
            );
        }
    }
}

pub struct BinaryServer {
    child: Child,
    pub address: SocketAddr,
    log: PathBuf,
}
impl BinaryServer {
    pub fn start(trusted: bool, seed: u64) -> Self {
        Self::start_with_options(trusted, seed, None, None)
    }
    pub fn start_with_diagnostics(trusted: bool, seed: u64, dir: Option<&std::path::Path>) -> Self {
        Self::start_with_options(trusted, seed, dir, None)
    }
    pub fn start_with_public_url(trusted: bool, seed: u64, public_url: Option<&str>) -> Self {
        Self::start_with_options(trusted, seed, None, public_url)
    }
    fn start_with_options(
        trusted: bool,
        seed: u64,
        dir: Option<&std::path::Path>,
        public_url: Option<&str>,
    ) -> Self {
        let log = std::env::temp_dir().join(format!(
            "ww2-r1-{}-{}.log",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut command = Command::new(env!("CARGO_BIN_EXE_ww2-server"));
        command
            .args(["--bind", "127.0.0.1:0"])
            .env_remove("PUBLIC_URL")
            .env_remove("WW2_BIND")
            .env_remove("WW2_DIAG")
            .env_remove("WW2_TICKLOG")
            .stdout(Stdio::piped())
            .stderr(File::create(&log).unwrap());
        if let Some(dir) = dir {
            command
                .current_dir(dir)
                .env("WW2_DIAG", "1")
                .env("WW2_TICKLOG", "1");
        }
        if trusted {
            command.args(["--trusted-room", "ABC", "--seed", &seed.to_string()]);
        }
        if let Some(public_url) = public_url {
            command.env("PUBLIC_URL", public_url);
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
            log,
        };
        let line = receiver
            .recv_timeout(Duration::from_secs(20))
            .expect("ordinary binary listening deadline")
            .unwrap();
        server.address = line
            .trim()
            .strip_prefix("LISTENING http://")
            .expect("ordinary binary address")
            .parse()
            .unwrap();
        server
    }
    pub fn log(&self) -> String {
        fs::read_to_string(&self.log).unwrap()
    }
    pub fn receipts(&self) -> Vec<Value> {
        complete_records(&fs::read(&self.log).unwrap(), b"NATIVE_MATCH ")
            .collect::<Result<_, _>>()
            .unwrap()
    }
    pub fn directory(&self) -> Value {
        let mut stream = TcpStream::connect(self.address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        write!(
            stream,
            "GET /api/rooms HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            self.address
        )
        .unwrap();
        let mut text = String::new();
        stream.read_to_string(&mut text).unwrap();
        let (_, body) = text.split_once("\r\n\r\n").unwrap();
        serde_json::from_str(body).unwrap()
    }
}
impl Drop for BinaryServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if std::thread::panicking() {
            eprintln!(
                "ordinary binary host log: {}\n{}",
                self.log.display(),
                self.log()
            );
        } else {
            let _ = fs::remove_file(&self.log);
        }
    }
}

pub struct Client {
    pub socket: TcpStream,
    pub view: ClientView,
    pub messages: Vec<Value>,
    marker: u64,
}
impl Client {
    pub fn open(address: SocketAddr, room: &str, hello: Value) -> Self {
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
        let mut client = Self {
            socket,
            view: ClientView::new(),
            messages: Vec::new(),
            marker: 0,
        };
        client.send(hello);
        client
    }
    pub fn join(address: SocketAddr, token: &str) -> Self {
        let mut client = Self::open(
            address,
            "ABC",
            json!({"t":"hello","name":token,"token":token,"proto":0}),
        );
        client.until(|m| m["t"] == "lobby");
        client
    }
    pub fn send(&mut self, message: Value) {
        let bytes = message.to_string().into_bytes();
        let mask = [13_u8, 17, 19, 23];
        let mut frame = vec![0x81];
        match bytes.len() {
            n if n < 126 => frame.push(0x80 | n as u8),
            n => {
                frame.push(0x80 | 126);
                frame.extend(u16::try_from(n).unwrap().to_be_bytes());
            }
        }
        frame.extend(mask);
        frame.extend(bytes.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
        self.socket.write_all(&frame).unwrap();
    }
    pub fn frame(&mut self) -> (u8, Vec<u8>) {
        let mut header = [0; 2];
        self.socket.read_exact(&mut header).unwrap();
        assert_eq!(header[1] & 0x80, 0);
        assert_ne!(header[0] & 0x80, 0);
        let size = match header[1] & 127 {
            126 => {
                let mut b = [0; 2];
                self.socket.read_exact(&mut b).unwrap();
                usize::from(u16::from_be_bytes(b))
            }
            127 => {
                let mut b = [0; 8];
                self.socket.read_exact(&mut b).unwrap();
                usize::try_from(u64::from_be_bytes(b)).unwrap()
            }
            n => usize::from(n),
        };
        assert!(size < 16 * 1024 * 1024);
        let mut body = vec![0; size];
        self.socket.read_exact(&mut body).unwrap();
        (header[0] & 15, body)
    }
    pub fn receive(&mut self) -> Value {
        let (kind, body) = self.frame();
        assert_eq!(kind, 1, "expected JSON text frame");
        let text = String::from_utf8(body).unwrap();
        let message: Value = serde_json::from_str(&text).unwrap();
        self.view
            .apply(&text)
            .unwrap_or_else(|e| panic!("actual ClientView rejected {}: {e}", message["t"]));
        assert!(
            message.get("seed").is_none()
                && message.get("weatherSeed").is_none()
                && message.get("options").is_none()
        );
        if message["t"] == "start" {
            assert_eq!(message.as_object().unwrap().len(), 13);
        }
        self.messages.push(message.clone());
        message
    }
    pub fn until(&mut self, wanted: impl Fn(&Value) -> bool) -> Value {
        for _ in 0..512 {
            let message = self.receive();
            if wanted(&message) {
                return message;
            }
        }
        panic!("expected packet not received within 512 messages");
    }
    pub fn barrier(&mut self) -> Vec<Value> {
        self.marker += 1;
        let marker = self.marker;
        let begin = self.messages.len();
        self.send(json!({"t":"ping","c":marker}));
        self.until(|m| m["t"] == "pong" && m["c"] == marker);
        self.messages[begin..].to_vec()
    }
    pub fn deny(&mut self, command: &str) -> Value {
        self.until(|m| m["t"] == "deny" && m["cmd"] == command)
    }
}
