use std::{
    collections::VecDeque,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::OnceLock,
    time::{Duration, Instant},
};

use serde_json::{Value, json};

const TIMEOUT: Duration = Duration::from_secs(10);

type TestResult<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn send_message(stdin: &mut impl Write, msg: &Value) -> TestResult {
    let body = serde_json::to_string(msg)?;
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    stdin.write_all(header.as_bytes())?;
    stdin.write_all(body.as_bytes())?;
    stdin.flush()?;
    Ok(())
}

fn read_message(reader: &mut BufReader<impl std::io::Read>) -> TestResult<Value> {
    let mut content_length: Option<usize> = None;

    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some(val) = trimmed.strip_prefix("Content-Length: ") {
            content_length = Some(val.parse()?);
        }
    }

    let len = content_length.ok_or("Content-Length header missing")?;
    let mut buf = vec![0u8; len];
    std::io::Read::read_exact(reader, &mut buf)?;
    let value: Value = serde_json::from_slice(&buf)?;
    Ok(value)
}

fn server_binary() -> TestResult<&'static PathBuf> {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    if let Some(path) = BINARY.get() {
        return Ok(path);
    }

    let project_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .ok_or("cannot resolve workspace root")?;

    let output = Command::new("cargo")
        .args([
            "build",
            "-p",
            "svg-language-server",
            "--message-format=json-render-diagnostics",
        ])
        .current_dir(project_root)
        .output()?;
    assert!(
        output.status.success(),
        "cargo build failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let binary = String::from_utf8(output.stdout)?
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find_map(|message| {
            let is_bin_target = message["target"]["kind"]
                .as_array()
                .is_some_and(|kinds| kinds.iter().any(|kind| kind.as_str() == Some("bin")));
            if message["reason"].as_str() == Some("compiler-artifact") && is_bin_target {
                return message["executable"].as_str().map(PathBuf::from);
            }
            None
        })
        .ok_or("cargo build did not report an executable artifact")?;
    assert!(binary.exists(), "binary not found at {}", binary.display());

    // If another thread raced us, `set` returns Err but the value is
    // still present via `get`, so both paths are fine.
    let _ = BINARY.set(binary);
    BINARY.get().ok_or_else(|| "OnceLock was not set".into())
}

pub struct TestServer {
    child: Child,
    stdin: Option<ChildStdin>,
    pub rx: std::sync::mpsc::Receiver<Value>,
    reader_thread: Option<std::thread::JoinHandle<()>>,
    next_id: u64,
    pub init_response: Value,
    pub response_buf: VecDeque<Value>,
    pub notification_buf: VecDeque<Value>,
}

impl TestServer {
    pub fn start() -> TestResult<Self> {
        Self::start_with_initialize_options(&Value::Null)
    }

    pub fn start_with_initialize_options(initialization_options: &Value) -> TestResult<Self> {
        let mut child = Command::new(server_binary()?)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;

        let stdin = child.stdin.take().ok_or("take stdin")?;
        let stdout = child.stdout.take().ok_or("take stdout")?;

        let (tx, rx) = std::sync::mpsc::channel::<Value>();
        let reader_thread = std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            while let Ok(msg) = read_message(&mut reader) {
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
            response_buf: VecDeque::new(),
            notification_buf: VecDeque::new(),
        };

        server.init_response = server.request(
            "initialize",
            &json!({
                "processId": null,
                "rootUri": null,
                "capabilities": {},
                "initializationOptions": initialization_options.clone()
            }),
        )?;
        server.notify("initialized", &json!({}))?;
        Ok(server)
    }

    pub fn notify(&mut self, method: &str, params: &Value) -> TestResult {
        send_message(
            self.stdin.as_mut().ok_or("stdin available")?,
            &json!({
                "jsonrpc": "2.0",
                "method": method,
                "params": params.clone(),
            }),
        )
    }

    pub fn request(&mut self, method: &str, params: &Value) -> TestResult<Value> {
        let id = self.next_id;
        self.next_id += 1;
        send_message(
            self.stdin.as_mut().ok_or("stdin available")?,
            &json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": method,
                "params": params.clone(),
            }),
        )?;
        self.recv_response(id)
    }

    pub fn open(&mut self, uri: &str, text: &str) -> TestResult {
        self.notify(
            "textDocument/didOpen",
            &json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": "svg",
                    "version": 1,
                    "text": text
                }
            }),
        )
    }

    #[allow(dead_code)]
    pub fn change_configuration(&mut self, settings: &Value) -> TestResult {
        self.notify(
            "workspace/didChangeConfiguration",
            &json!({
                "settings": settings.clone(),
            }),
        )
    }

    pub fn shutdown_and_exit(&mut self) -> TestResult {
        let response = self.request("shutdown", &Value::Null)?;
        assert!(
            response.get("result").is_some(),
            "shutdown should return a result: {response}"
        );
        self.notify("exit", &Value::Null)?;

        drop(self.stdin.take());
        let status = self.child.wait()?;
        assert!(
            status.success(),
            "server exited with non-zero status: {status}"
        );

        if let Some(reader_thread) = self.reader_thread.take() {
            let _ = reader_thread.join();
        }
        Ok(())
    }

    /// Wait for a response with the given ID, buffering any notifications
    /// that arrive in the meantime so they aren't lost.
    fn recv_response(&mut self, expected_id: u64) -> TestResult<Value> {
        // Check buffered responses first.
        if let Some(idx) = self
            .response_buf
            .iter()
            .position(|msg| msg.get("id").and_then(Value::as_u64) == Some(expected_id))
        {
            return Ok(self
                .response_buf
                .remove(idx)
                .unwrap_or_else(|| unreachable!()));
        }

        let deadline = Instant::now() + TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(
                !remaining.is_zero(),
                "timed out waiting for response with id {expected_id}"
            );
            match self.rx.recv_timeout(remaining) {
                Ok(msg) => {
                    if msg.get("id").and_then(Value::as_u64) == Some(expected_id) {
                        return Ok(msg);
                    }
                    // Buffer the message in the appropriate queue.
                    if msg.get("id").is_some() {
                        self.response_buf.push_back(msg);
                    } else {
                        self.notification_buf.push_back(msg);
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
