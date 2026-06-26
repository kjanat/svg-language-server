use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let src_dir = std::path::Path::new("src");

    let mut c_config = cc::Build::new();
    c_config.std("c11").include(src_dir);

    #[cfg(target_env = "msvc")]
    c_config.flag("-utf-8");

    if std::env::var("TARGET")? == "wasm32-unknown-unknown" {
        let wasm_headers =
            std::env::var("DEP_TREE_SITTER_LANGUAGE_WASM_HEADERS").map_err(|err| {
                format!(
                    "DEP_TREE_SITTER_LANGUAGE_WASM_HEADERS must be set by the language crate: \
                     {err}"
                )
            })?;
        let wasm_src = std::env::var("DEP_TREE_SITTER_LANGUAGE_WASM_SRC")
            .map(std::path::PathBuf::from)
            .map_err(|err| {
                format!(
                    "DEP_TREE_SITTER_LANGUAGE_WASM_SRC must be set by the language crate: {err}"
                )
            })?;

        c_config.include(&wasm_headers);
        c_config.files([
            wasm_src.join("stdio.c"),
            wasm_src.join("stdlib.c"),
            wasm_src.join("string.c"),
        ]);
    }

    let parser_path = src_dir.join("parser.c");
    c_config.file(&parser_path);
    println!("cargo:rerun-if-changed={}", parser_path.display());

    let scanner_path = src_dir.join("scanner.c");
    if scanner_path.exists() {
        c_config.file(&scanner_path);
        println!("cargo:rerun-if-changed={}", scanner_path.display());
    }

    c_config.compile("tree-sitter-svg-paint");

    emit_query_cfg("queries/highlights.scm", "with_highlights_query");
    emit_query_cfg("queries/injections.scm", "with_injections_query");
    emit_query_cfg("queries/locals.scm", "with_locals_query");
    emit_query_cfg("queries/tags.scm", "with_tags_query");

    Ok(())
}

fn emit_query_cfg(path: &str, cfg: &str) {
    println!("cargo:rerun-if-changed={path}");
    println!("cargo:rustc-check-cfg=cfg({cfg})");
    if std::path::Path::new(path).exists() {
        println!("cargo:rustc-cfg={cfg}");
    }
}
