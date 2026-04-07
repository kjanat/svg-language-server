//! Binary entrypoint for the `svg-language-server` executable.

#[tokio::main]
async fn main() {
    svg_language_server::run_stdio_server().await;
}
