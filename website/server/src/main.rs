//! Std-only file server for the Lastdown anthology.
//!
//! The binary is a separate crate. It does not depend on the Lastdown library
//! and it does not sit in that library's workspace.

#![forbid(unsafe_code)]

use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

const DEFAULT_PORT: u16 = 8741;
const MAX_HEADER: usize = 8 * 1024;

fn main() {
    let root = match anthology_root().canonicalize() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("lastdown anthology root: {error}");
            std::process::exit(1);
        }
    };
    let port = env::var("ANTHOLOGY_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_PORT);
    let bind = env::var("ANTHOLOGY_BIND").unwrap_or_else(|_| "127.0.0.1".to_string());
    let addr = format!("{bind}:{port}");
    let listener = match TcpListener::bind(&addr) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("lastdown anthology failed to bind {addr}: {error}");
            std::process::exit(1);
        }
    };
    let bound = listener.local_addr().expect("bound address");
    eprintln!("lastdown anthology http://{bound}/");
    eprintln!("root {}", root.display());
    for incoming in listener.incoming() {
        let Ok(stream) = incoming else {
            continue;
        };
        let root = root.clone();
        std::thread::spawn(move || {
            if let Err(error) = serve(stream, &root) {
                eprintln!("lastdown anthology request: {error}");
            }
        });
    }
}

fn anthology_root() -> PathBuf {
    env::var_os("ANTHOLOGY_ROOT").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../public"),
        PathBuf::from,
    )
}

fn serve(mut stream: TcpStream, root: &Path) -> std::io::Result<()> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(8)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(8)));
    let mut buffer = [0_u8; MAX_HEADER];
    let mut filled = 0;
    while filled < MAX_HEADER {
        match stream.read(&mut buffer[filled..]) {
            Ok(0) => break,
            Ok(count) => {
                filled += count;
                if buffer[..filled]
                    .windows(4)
                    .any(|window| window == b"\r\n\r\n")
                {
                    break;
                }
            }
            Err(error) => return Err(error),
        }
    }
    let text = String::from_utf8_lossy(&buffer[..filled]);
    let Some(line) = text.lines().next() else {
        return write_bytes(
            &mut stream,
            "400 Bad Request",
            "text/plain; charset=utf-8",
            b"bad request",
            true,
        );
    };
    let mut parts = line.split_whitespace();
    let Some(method) = parts.next() else {
        return write_bytes(
            &mut stream,
            "400 Bad Request",
            "text/plain; charset=utf-8",
            b"bad request",
            true,
        );
    };
    let Some(raw_path) = parts.next() else {
        return write_bytes(
            &mut stream,
            "400 Bad Request",
            "text/plain; charset=utf-8",
            b"bad request",
            true,
        );
    };
    if method != "GET" && method != "HEAD" {
        return write_bytes(
            &mut stream,
            "405 Method Not Allowed",
            "text/plain; charset=utf-8",
            b"method not allowed",
            method != "HEAD",
        );
    }
    match resolve(root, raw_path) {
        Target::Ok(path) => {
            let body = fs::read(&path)?;
            let kind = content_type(&path);
            write_bytes(&mut stream, "200 OK", kind, &body, method != "HEAD")
        }
        Target::Missing => {
            let missing = missing_page(root);
            write_bytes(
                &mut stream,
                "404 Not Found",
                "text/html; charset=utf-8",
                &missing,
                method != "HEAD",
            )
        }
        Target::Bad => write_bytes(
            &mut stream,
            "400 Bad Request",
            "text/plain; charset=utf-8",
            b"bad request",
            method != "HEAD",
        ),
    }
}

fn missing_page(root: &Path) -> Vec<u8> {
    let path = root.join("404.html");
    fs::read(path).unwrap_or_else(|_| {
        b"<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>Missing plate</title></head><body><h1>Missing plate</h1><p><a href=\"/\">Contents</a></p></body></html>".to_vec()
    })
}

fn write_bytes(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
    include_body: bool,
) -> std::io::Result<()> {
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\nContent-Security-Policy: default-src 'self'\r\nReferrer-Policy: no-referrer\r\nCache-Control: no-cache\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    if include_body {
        stream.write_all(body)?;
    }
    stream.flush()
}

fn content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
    {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "svg" => "image/svg+xml",
        "json" => "application/json",
        "txt" => "text/plain; charset=utf-8",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        _ => "application/octet-stream",
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Target {
    Ok(PathBuf),
    Missing,
    Bad,
}

fn resolve(root: &Path, raw_path: &str) -> Target {
    let decoded = match percent_decode(raw_path) {
        Ok(value) => value,
        Err(()) => return Target::Bad,
    };
    let path_only = decoded.split(['?', '#']).next().unwrap_or("/");
    if !path_only.starts_with('/') || path_only.contains('\\') {
        return Target::Bad;
    }
    let mut relative = PathBuf::new();
    for component in Path::new(path_only).components() {
        match component {
            Component::RootDir | Component::CurDir => {}
            Component::Normal(part) => relative.push(part),
            Component::ParentDir | Component::Prefix(_) => return Target::Bad,
        }
    }
    let mut candidate = root.join(&relative);
    if path_only.ends_with('/') || candidate.is_dir() {
        candidate.push("index.html");
    } else if candidate.extension().is_none() {
        let html = candidate.with_extension("html");
        if html.is_file() {
            candidate = html;
        }
    }
    if !candidate.is_file() {
        return Target::Missing;
    }
    match (root.canonicalize(), candidate.canonicalize()) {
        (Ok(root_real), Ok(file_real)) if file_real.starts_with(&root_real) => {
            Target::Ok(file_real)
        }
        _ => Target::Bad,
    }
}

fn percent_decode(input: &str) -> Result<String, ()> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err(());
            }
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).map_err(|_| ())?;
            out.push(u8::from_str_radix(hex, 16).map_err(|_| ())?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("lastdown-anthology-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("css")).unwrap();
        fs::write(dir.join("index.html"), b"home").unwrap();
        fs::write(dir.join("field.html"), b"field").unwrap();
        fs::write(dir.join("css").join("anthology.css"), b"css").unwrap();
        dir.canonicalize().unwrap()
    }

    #[test]
    fn plates_resolve_under_the_root() {
        let root = scratch("plates");
        match resolve(&root, "/") {
            Target::Ok(path) => assert!(path.ends_with("index.html")),
            other => panic!("index: {other:?}"),
        }
        match resolve(&root, "/field") {
            Target::Ok(path) => assert!(path.ends_with("field.html")),
            other => panic!("field: {other:?}"),
        }
        match resolve(&root, "/css/anthology.css") {
            Target::Ok(path) => assert!(path.ends_with("anthology.css")),
            other => panic!("css: {other:?}"),
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn traversal_is_a_bad_request() {
        let root = scratch("traversal");
        assert_eq!(resolve(&root, "/../Cargo.toml"), Target::Bad);
        assert_eq!(resolve(&root, "/%2e%2e/Cargo.toml"), Target::Bad);
        assert_eq!(resolve(&root, "/missing-plate"), Target::Missing);
        let _ = fs::remove_dir_all(root);
    }
}
