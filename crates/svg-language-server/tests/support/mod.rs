use std::{
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::OnceLock,
    time::{Duration, Instant},
};

use serde_json::{Value, json};

const TIMEOUT: Duration = Duration::from_secs(10);

fn send_message(stdin: &mut impl Write, msg: &Value) {
    let body = serde_json::to_string(msg).expect("serialize JSON-RPC message");
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    stdin
        .write_all(header.as_bytes())
        .expect("write header to stdin");
    stdin
        .write_all(body.as_bytes())
        .expect("write body to stdin");
    stdin.flush().expect("flush stdin");
}

fn read_message(reader: &mut BufReader<impl std::io::Read>) -> Value {
    let mut content_length: Option<usize> = None;

    loop {
        let mut line = String::new();
        reader.read_line(&mut line).expect("read header line");
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some(val) = trimmed.strip_prefix("Content-Length: ") {
            content_length = Some(val.parse().expect("parse Content-Length"));
        }
    }

    let len = content_length.expect("Content-Length header missing");
    let mut buf = vec![0u8; len];
    std::io::Read::read_exact(reader, &mut buf).expect("read message body");
    serde_json::from_slice(&buf).expect("parse JSON body")
}

fn server_binary() -> &'static PathBuf {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY.get_or_init(|| {
        let project_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("resolve workspace root");

        let status = Command::new("cargo")
            .args(["build", "-p", "svg-language-server"])
            .current_dir(project_root)
            .status()
            .expect("run cargo build");
        assert!(status.success(), "cargo build failed");

        let exe_name = if std::env::consts::EXE_EXTENSION.is_empty() {
            "svg-language-server".to_string()
        } else {
            format!("svg-language-server.{}", std::env::consts::EXE_EXTENSION)
        };
        let binary = project_root.join("target/debug").join(exe_name);
        assert!(binary.exists(), "binary not found at {}", binary.display());
        binary
    })
}

pub struct TestServer {
    child: Child,
    stdin: Option<ChildStdin>,
    pub rx: std::sync::mpsc::Receiver<Value>,
    reader_thread: Option<std::thread::JoinHandle<()>>,
    next_id: u64,
    pub init_response: Value,
}

impl TestServer {
    pub fn new() -> Self {
        let mut child = Command::new(server_binary())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn svg-language-server");

        let stdin = child.stdin.take().expect("take stdin");
        let stdout = child.stdout.take().expect("take stdout");

        let (tx, rx) = std::sync::mpsc::channel::<Value>();
        let reader_thread = std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let msg = read_message(&mut reader);
                if tx.send(msg).is_err() {
                    break;
                }
            }
        });

        let mut server = Self {
            child,
            stdin: Some(stdin),
            rx,
            reader_thread: Some(reader_thread),
            next_id: 1,
            init_response: Value::Null,
        };

        server.init_response = server.request(
            "initialize",
            json!({
                "processId": null,
                "rootUri": null,
                "capabilities": {}
            }),
        );
        server.notify("initialized", json!({}));
        server
    }

    pub fn notify(&mut self, method: &str, params: Value) {
        send_message(
            self.stdin.as_mut().expect("stdin available"),
            &json!({
                "jsonrpc": "2.0",
                "method": method,
                "params": params,
            }),
        );
    }

    pub fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        send_message(
            self.stdin.as_mut().expect("stdin available"),
            &json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": method,
                "params": params,
            }),
        );
        self.recv_response(id)
    }

    pub fn open(&mut self, uri: &str, text: &str) {
        self.notify(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": "svg",
                    "version": 1,
                    "text": text
                }
            }),
        );
    }

    pub fn shutdown_and_exit(&mut self) {
        let response = self.request("shutdown", Value::Null);
        assert!(
            response.get("result").is_some(),
            "shutdown should return a result: {response}"
        );
        self.notify("exit", Value::Null);

        drop(self.stdin.take());
        let status = self.child.wait().expect("wait for server process");
        assert!(
            status.success(),
            "server exited with non-zero status: {status}"
        );

        if let Some(reader_thread) = self.reader_thread.take() {
            let _ = reader_thread.join();
        }
    }

    fn recv_response(&mut self, expected_id: u64) -> Value {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                panic!("timed out waiting for response with id {expected_id}");
            }
            match self.rx.recv_timeout(remaining) {
                Ok(msg) => {
                    if msg.get("id").and_then(Value::as_u64) == Some(expected_id) {
                        return msg;
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    panic!("timed out waiting for response with id {expected_id}");
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    panic!("reader thread disconnected while waiting for id {expected_id}");
                }
            }
        }
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        drop(self.stdin.take());

        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }

        if let Some(reader_thread) = self.reader_thread.take() {
            let _ = reader_thread.join();
        }
    }
}
