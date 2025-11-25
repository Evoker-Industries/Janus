//! HTTP Server implementation

use crate::proxy::ProxyHandler;
use crate::AppState;
use anyhow::Result;
use bytes::Bytes;
use http_body_util::{combinators::BoxBody, BodyExt, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{debug, error, info, warn};

/// Run the HTTP server
pub async fn run_server(state: Arc<AppState>) -> Result<()> {
    let config = state.config.read().await;
    let addr: SocketAddr =
        format!("{}:{}", config.server.bind_address, config.server.port).parse()?;
    drop(config);

    let listener = TcpListener::bind(addr).await?;
    info!("HTTP server listening on http://{}", addr);

    loop {
        let (stream, remote_addr) = listener.accept().await?;
        let io = TokioIo::new(stream);
        let state = state.clone();

        tokio::spawn(async move {
            let service = service_fn(move |req| {
                let state = state.clone();
                async move { handle_request(state, req, remote_addr).await }
            });

            if let Err(err) = http1::Builder::new().serve_connection(io, service).await {
                debug!("Connection error: {:?}", err);
            }
        });
    }
}

/// Handle incoming HTTP request
async fn handle_request(
    state: Arc<AppState>,
    req: Request<Incoming>,
    remote_addr: SocketAddr,
) -> Result<Response<BoxBody<Bytes, Infallible>>, Infallible> {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let path = uri.path();

    // Update stats
    {
        let mut stats = state.stats.write().await;
        stats.total_requests += 1;
    }

    let config = state.config.read().await;

    if config.server.access_log {
        info!(
            "{} {} {} - {}",
            remote_addr.ip(),
            method,
            path,
            uri.query().unwrap_or("")
        );
    }

    // Try to match static file routes first
    for static_config in &config.static_files {
        if path.starts_with(&static_config.path) {
            let file_path = path.strip_prefix(&static_config.path).unwrap_or(path);
            let file_path = if file_path.is_empty() || file_path == "/" {
                &static_config.index
            } else {
                file_path.trim_start_matches('/')
            };

            let full_path = std::path::Path::new(&static_config.root).join(file_path);

            if full_path.is_file() {
                match tokio::fs::read(&full_path).await {
                    Ok(contents) => {
                        let content_type = guess_content_type(&full_path);
                        let response = Response::builder()
                            .status(StatusCode::OK)
                            .header("Content-Type", content_type)
                            .body(full_body(contents))
                            .unwrap();

                        update_status_stats(&state, StatusCode::OK).await;
                        return Ok(response);
                    }
                    Err(e) => {
                        warn!("Failed to read file {:?}: {}", full_path, e);
                    }
                }
            } else if static_config.directory_listing && full_path.is_dir() {
                let listing = generate_directory_listing(&full_path, path).await;
                let response = Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", "text/html")
                    .body(full_body(listing.into_bytes()))
                    .unwrap();

                update_status_stats(&state, StatusCode::OK).await;
                return Ok(response);
            }
        }
    }

    // Try to match proxy routes
    for route in &config.routes {
        if matches_route(path, &route.path) {
            // Check method if specified
            if !route.methods.is_empty() {
                let method_str = method.as_str().to_uppercase();
                if !route.methods.iter().any(|m| m.to_uppercase() == method_str) {
                    continue;
                }
            }

            // Find upstream
            if let Some(upstream) = config.upstreams.get(&route.upstream) {
                let proxy = ProxyHandler::new(upstream.clone(), route.clone());
                drop(config);

                match proxy.forward(req, remote_addr).await {
                    Ok(response) => {
                        let status = response.status();
                        update_status_stats(&state, status).await;
                        return Ok(response);
                    }
                    Err(e) => {
                        error!("Proxy error: {}", e);
                        let response = error_response(StatusCode::BAD_GATEWAY, "Bad Gateway");
                        update_status_stats(&state, StatusCode::BAD_GATEWAY).await;
                        return Ok(response);
                    }
                }
            } else {
                warn!(
                    "Upstream '{}' not found for route '{}'",
                    route.upstream, route.path
                );
            }
        }
    }

    drop(config);

    // No route matched - return 404
    let response = error_response(StatusCode::NOT_FOUND, "Not Found");
    update_status_stats(&state, StatusCode::NOT_FOUND).await;
    Ok(response)
}

/// Check if path matches route pattern
fn matches_route(path: &str, pattern: &str) -> bool {
    if pattern.ends_with("/*") {
        let prefix = pattern.trim_end_matches("/*");
        path.starts_with(prefix)
    } else if pattern.ends_with('*') {
        let prefix = pattern.trim_end_matches('*');
        path.starts_with(prefix)
    } else {
        path == pattern
    }
}

/// Update status code statistics
async fn update_status_stats(state: &Arc<AppState>, status: StatusCode) {
    let mut stats = state.stats.write().await;
    let code = status.as_u16();
    if (200..300).contains(&code) {
        stats.status_codes.success += 1;
    } else if (300..400).contains(&code) {
        stats.status_codes.redirect += 1;
    } else if (400..500).contains(&code) {
        stats.status_codes.client_error += 1;
    } else if code >= 500 {
        stats.status_codes.server_error += 1;
    }
}

/// Guess content type from file extension
fn guess_content_type(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") | Some("htm") => "text/html",
        Some("css") => "text/css",
        Some("js") => "application/javascript",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("txt") => "text/plain",
        Some("xml") => "application/xml",
        Some("pdf") => "application/pdf",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        _ => "application/octet-stream",
    }
}

/// Generate directory listing HTML
async fn generate_directory_listing(dir: &std::path::Path, url_path: &str) -> String {
    let mut entries = Vec::new();

    if let Ok(mut read_dir) = tokio::fs::read_dir(dir).await {
        while let Ok(Some(entry)) = read_dir.next_entry().await {
            if let Ok(name) = entry.file_name().into_string() {
                let is_dir = entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false);
                entries.push((name, is_dir));
            }
        }
    }

    entries.sort_by(|a, b| match (a.1, b.1) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.0.cmp(&b.0),
    });

    let mut html = format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <title>Index of {}</title>
    <style>
        body {{ font-family: monospace; padding: 20px; }}
        a {{ text-decoration: none; }}
        a:hover {{ text-decoration: underline; }}
        .dir {{ color: blue; }}
        .file {{ color: black; }}
    </style>
</head>
<body>
    <h1>Index of {}</h1>
    <hr>
    <ul>
"#,
        url_path, url_path
    );

    // Parent directory link
    if url_path != "/" {
        html.push_str(r#"        <li><a href="..">..</a></li>\n"#);
    }

    for (name, is_dir) in entries {
        let class = if is_dir { "dir" } else { "file" };
        let suffix = if is_dir { "/" } else { "" };
        html.push_str(&format!(
            r#"        <li><a class="{}" href="{}{}">{}{}</a></li>\n"#,
            class, name, suffix, name, suffix
        ));
    }

    html.push_str(
        r#"    </ul>
    <hr>
    <p>Janus Server</p>
</body>
</html>"#,
    );

    html
}

/// Create a full body response
fn full_body(data: Vec<u8>) -> BoxBody<Bytes, Infallible> {
    Full::new(Bytes::from(data))
        .map_err(|_| unreachable!())
        .boxed()
}

/// Create an error response
fn error_response(status: StatusCode, message: &str) -> Response<BoxBody<Bytes, Infallible>> {
    let body = format!(
        r#"<!DOCTYPE html>
<html>
<head><title>{} {}</title></head>
<body>
    <h1>{} {}</h1>
    <hr>
    <p>Janus Server</p>
</body>
</html>"#,
        status.as_u16(),
        message,
        status.as_u16(),
        message
    );

    Response::builder()
        .status(status)
        .header("Content-Type", "text/html")
        .body(full_body(body.into_bytes()))
        .unwrap()
}
