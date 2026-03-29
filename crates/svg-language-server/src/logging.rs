use super::*;
use std::fs::OpenOptions;
use std::path::PathBuf;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{Layer, Registry};

pub(crate) struct LoggingGuards {
    pub(crate) _file_guard: Option<tracing_appender::non_blocking::WorkerGuard>,
    pub(crate) _stderr_guard: tracing_appender::non_blocking::WorkerGuard,
}

fn default_log_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("SVG_LS_LOG_DIR") {
        return PathBuf::from(path);
    }
    if let Some(path) = std::env::var_os("XDG_CACHE_HOME") {
        return PathBuf::from(path).join("svg-language-server");
    }
    if let Some(path) = std::env::var_os("HOME") {
        return PathBuf::from(path)
            .join(".cache")
            .join("svg-language-server");
    }
    if let Some(path) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(path).join("svg-language-server");
    }
    std::env::temp_dir().join("svg-language-server")
}

fn install_panic_hook() {
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let location = panic_info
            .location()
            .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()))
            .unwrap_or_else(|| "unknown location".to_string());

        let payload = panic_info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| {
                panic_info
                    .payload()
                    .downcast_ref::<String>()
                    .map(String::as_str)
            })
            .unwrap_or("non-string panic payload");

        tracing::error!(target: "svg_language_server::panic", %location, %payload, "panic");
        eprintln!("svg-language-server panic at {location}: {payload}");

        previous_hook(panic_info);
    }));
}

pub(crate) fn init_logging() -> LoggingGuards {
    let log_dir = default_log_dir();
    let stderr_appender = tracing_appender::non_blocking(std::io::stderr());
    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_writer(stderr_appender.0.clone())
        .with_target(true)
        .with_filter(LevelFilter::INFO)
        .boxed();

    let mut file_guard = None;
    let mut file_log_path = None;
    let mut file_layer = None;

    if fs::create_dir_all(&log_dir).is_ok() {
        let path = log_dir.join("server.log");
        if let Ok(file) = OpenOptions::new().create(true).append(true).open(&path) {
            let non_blocking = tracing_appender::non_blocking(file);
            file_log_path = Some(path);
            file_guard = Some(non_blocking.1);
            file_layer = Some(
                tracing_subscriber::fmt::layer()
                    .with_writer(non_blocking.0)
                    .with_ansi(false)
                    .with_target(true)
                    .with_filter(LevelFilter::DEBUG)
                    .boxed(),
            );
        }
    }

    let subscriber = Registry::default().with(stderr_layer).with(file_layer);

    if let Err(err) = tracing::subscriber::set_global_default(subscriber) {
        eprintln!("svg-language-server failed to initialize tracing subscriber: {err}");
    }

    install_panic_hook();

    if let Some(path) = &file_log_path {
        tracing::info!(log_file = %path.display(), "logging initialized");
    } else {
        tracing::warn!("logging initialized without file sink");
    }

    LoggingGuards {
        _file_guard: file_guard,
        _stderr_guard: stderr_appender.1,
    }
}
