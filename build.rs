use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=BHASA_LEXER_ARCHIVE");
    if env::var_os("CARGO_FEATURE_BHASA_LEXER").is_none() {
        return;
    }
    let host = env::var("HOST").expect("Cargo did not provide HOST");
    let target = env::var("TARGET").expect("Cargo did not provide TARGET");
    assert_eq!(
        host, target,
        "bhasa-lexer currently requires a host build; cross-compiling the Rust host with a native lexer archive is unsupported"
    );
    assert!(
        target.contains("apple-darwin") || target.ends_with("linux-gnu"),
        "bhasa-lexer currently supports macOS and GNU/Linux hosts"
    );
    let archive = PathBuf::from(env::var_os("BHASA_LEXER_ARCHIVE").expect(
        "bhasa-lexer requires BHASA_LEXER_ARCHIVE; run sh bootstrap/build-hybrid.sh to build the Bhasa scanner archive",
    ));
    assert!(
        archive.is_absolute() && archive.is_file(),
        "BHASA_LEXER_ARCHIVE must name an existing absolute static library path"
    );
    let archive = archive
        .canonicalize()
        .expect("cannot resolve lexer archive");
    let archive_text = archive.to_str().expect("lexer archive path must be UTF-8");
    assert!(
        !archive_text.contains(['\n', '\r']),
        "lexer archive path cannot contain newlines"
    );
    let stamp = archive.with_extension("target");
    let archive_target = fs::read_to_string(&stamp)
        .expect("lexer archive needs an adjacent .target file; run sh bootstrap/build-hybrid.sh");
    assert_eq!(
        archive_target.trim(),
        target,
        "Bhasa lexer archive target does not match the Rust host"
    );
    if env::var_os("CARGO_FEATURE_BHASA_FRONTEND").is_some() {
        let frontend_stamp = archive.with_extension("frontend");
        assert!(
            fs::read_to_string(&frontend_stamp).is_ok_and(|value| value.trim() == "bhasa-frontend-v2"),
            "bhasa-frontend requires a complete frontend archive; run sh bootstrap/build-hybrid.sh --frontend"
        );
        println!("cargo:rerun-if-changed={}", frontend_stamp.display());
    }
    let filename = archive.file_name().unwrap().to_str().unwrap();
    let name = filename
        .strip_prefix("lib")
        .and_then(|name| name.strip_suffix(".a"))
        .filter(|name| {
            !name.is_empty()
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        })
        .expect("lexer archive must have a name such as libbhasa_lexer.a");
    println!("cargo:rerun-if-changed={archive_text}");
    println!("cargo:rerun-if-changed={}", stamp.display());
    println!(
        "cargo:rustc-link-search=native={}",
        archive.parent().unwrap().display()
    );
    println!("cargo:rustc-link-lib=static={name}");
    if target.ends_with("linux-gnu") {
        println!("cargo:rustc-link-lib=pthread");
    }
}
