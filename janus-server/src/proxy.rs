//! Reverse proxy handler

use anyhow::Result;
use bytes::Bytes;
use http_body_util::{combinators::BoxBody, BodyExt, Full};
use hyper::body::Incoming;
use hyper::{Request, Response, StatusCode};
use janus_common::config::{LoadBalancing, RouteConfig, UpstreamConfig};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use tracing::{debug, error};

/// Proxy handler for forwarding requests to upstream servers
pub struct ProxyHandler {
    upstream: UpstreamConfig,
    route: RouteConfig,
    counter: AtomicUsize,
}

impl ProxyHandler {
    pub fn new(upstream: UpstreamConfig, route: RouteConfig) -> Self {
        Self {
            upstream,
            route,
            counter: AtomicUsize::new(0),
        }
    }

    /// Forward request to upstream server
    pub async fn forward(
        &self,
        req: Request<Incoming>,
        _remote_addr: SocketAddr,
    ) -> Result<Response<BoxBody<Bytes, Infallible>>> {
        // Select backend server
        let backend = self.select_backend()?;
        
        // Build upstream URL
        let path = req.uri().path();
        let query = req.uri().query().map(|q| format!("?{}", q)).unwrap_or_default();
        
        // Apply path rewrite if configured
        let upstream_path = if let Some(ref rewrite) = self.route.rewrite {
            apply_rewrite(path, &self.route.path, rewrite)
        } else {
            path.to_string()
        };
        
        let upstream_url = format!("http://{}{}{}", backend, upstream_path, query);
        debug!("Proxying to {}", upstream_url);

        // Build request to upstream
        let method = req.method().clone();
        let mut builder = Request::builder()
            .method(method)
            .uri(&upstream_url);

        // Copy headers (except host)
        for (name, value) in req.headers() {
            if name != hyper::header::HOST {
                builder = builder.header(name, value);
            }
        }

        // Add custom headers from route config
        for (name, value) in &self.route.headers {
            builder = builder.header(name.as_str(), value.as_str());
        }

        // Set host header to upstream
        let host = backend.split(':').next().unwrap_or(&backend);
        builder = builder.header(hyper::header::HOST, host);

        // Collect body
        let body_bytes = req.collect().await?.to_bytes();
        let body = Full::new(body_bytes);
        let upstream_req = builder.body(body)?;

        // Create HTTP client and send request
        let client = hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
            .build_http();

        let timeout = std::time::Duration::from_secs(self.route.timeout);
        
        match tokio::time::timeout(timeout, client.request(upstream_req)).await {
            Ok(Ok(response)) => {
                let status = response.status();
                let headers = response.headers().clone();
                let body_bytes = response.collect().await?.to_bytes();
                
                let mut builder = Response::builder().status(status);
                for (name, value) in headers {
                    if let Some(name) = name {
                        builder = builder.header(name, value);
                    }
                }
                
                let response = builder
                    .body(Full::new(body_bytes).map_err(|_: Infallible| unreachable!()).boxed())
                    .unwrap();
                
                Ok(response)
            }
            Ok(Err(e)) => {
                error!("Upstream request failed: {}", e);
                Ok(error_response(StatusCode::BAD_GATEWAY, "Bad Gateway"))
            }
            Err(_) => {
                error!("Upstream request timed out");
                Ok(error_response(StatusCode::GATEWAY_TIMEOUT, "Gateway Timeout"))
            }
        }
    }

    /// Select a backend server based on load balancing strategy
    fn select_backend(&self) -> Result<&str> {
        let servers: Vec<_> = self.upstream.servers.iter()
            .filter(|s| !s.backup)
            .collect();

        if servers.is_empty() {
            // Fall back to backup servers
            let backups: Vec<_> = self.upstream.servers.iter()
                .filter(|s| s.backup)
                .collect();
            
            if backups.is_empty() {
                anyhow::bail!("No backend servers available");
            }
            
            return Ok(&backups[0].address);
        }

        let server = match self.upstream.load_balancing {
            LoadBalancing::RoundRobin => {
                let idx = self.counter.fetch_add(1, Ordering::Relaxed) % servers.len();
                &servers[idx].address
            }
            LoadBalancing::Random => {
                use std::time::{SystemTime, UNIX_EPOCH};
                let seed = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .subsec_nanos() as usize;
                let idx = seed % servers.len();
                &servers[idx].address
            }
            LoadBalancing::LeastConnections | LoadBalancing::IpHash => {
                // For simplicity, fall back to round-robin for these strategies
                let idx = self.counter.fetch_add(1, Ordering::Relaxed) % servers.len();
                &servers[idx].address
            }
        };

        Ok(server)
    }
}

/// Apply path rewrite rule
fn apply_rewrite(path: &str, pattern: &str, rewrite: &str) -> String {
    // Simple rewrite: remove the matched prefix and prepend the rewrite prefix
    let prefix = pattern.trim_end_matches("/*").trim_end_matches('*');
    let suffix = path.strip_prefix(prefix).unwrap_or(path);
    format!("{}{}", rewrite.trim_end_matches('/'), suffix)
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
        .body(Full::new(Bytes::from(body)).map_err(|_: Infallible| unreachable!()).boxed())
        .unwrap()
}
