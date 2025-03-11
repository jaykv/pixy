use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer, Error};
use actix_web::http::StatusCode;
use futures::TryStreamExt;
use std::env;
use log::{info, error};
use env_logger;
use reqwest::{Client, ClientBuilder};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::str::FromStr;

// Configuration struct
struct ProxyConfig {
    upstream_url: String,
    client_cert_path: Option<String>,
    client_key_path: Option<String>,
    ca_cert_path: Option<String>,
}

impl ProxyConfig {
    fn from_env() -> Self {
        let upstream_url = env::var("UPSTREAM_URL")
            .expect("UPSTREAM_URL must be set");
        let client_cert_path = env::var("CLIENT_CERT_PATH").ok();
        let client_key_path = env::var("CLIENT_KEY_PATH").ok();
        let ca_cert_path = env::var("CA_CERT_PATH").ok();

        ProxyConfig {
            upstream_url,
            client_cert_path,
            client_key_path,
            ca_cert_path,
        }
    }
}

// Create HTTP client with TLS configuration
fn create_client(config: &ProxyConfig) -> Client {
    let mut client_builder = ClientBuilder::new()
        .use_native_tls() // Use native TLS instead of rustls to avoid version conflicts
        .timeout(std::time::Duration::from_secs(60));
    
    // Configure client certificate if provided, otherwise skip
    if let (Some(cert_path), Some(key_path)) = (&config.client_cert_path, &config.client_key_path) {
        let cert = std::fs::read(cert_path).expect("Failed to read client certificate");
        let key = std::fs::read(key_path).expect("Failed to read client key");
        
        // Correctly concatenate the cert and key
        let mut identity_data = Vec::new();
        identity_data.extend_from_slice(&cert);
        identity_data.extend_from_slice(&key);

        client_builder = client_builder
            .identity(reqwest::Identity::from_pem(&identity_data)
                .expect("Failed to create identity from PEM"));
    }
    
    // Add custom CA certificate if provided
    if let Some(ca_path) = &config.ca_cert_path {
        let ca_cert = std::fs::read(ca_path).expect("Failed to read CA certificate");
        client_builder = client_builder
            .add_root_certificate(
                reqwest::Certificate::from_pem(&ca_cert)
                    .expect("Failed to create certificate from PEM")
            );
    }
    
    client_builder.build().expect("Failed to build HTTP client")
}

// Main proxy handler for all HTTP methods
async fn proxy_handler(
    req: HttpRequest,
    body: web::Bytes,
    client: web::Data<Client>,
    config: web::Data<ProxyConfig>,
) -> Result<HttpResponse, Error> {
    let path = req.uri().path_and_query()
        .map(|p| p.as_str())
        .unwrap_or("");
    
    // Build the upstream URL
    let upstream_url = format!("{}{}", config.upstream_url, path);
    info!("Proxying request to: {}", upstream_url);
    
    // Create forwarded request with same method
    let mut request_builder = client.request(
        reqwest::Method::from_str(req.method().as_str())
            .map_err(|e| actix_web::error::ErrorBadRequest(e))?,
        &upstream_url
    );
    
    // Copy headers from original request
    let mut headers = HeaderMap::new();
    for (name, value) in req.headers() {
        if name.as_str().to_lowercase() != "host" && 
           name.as_str().to_lowercase() != "connection" {
               if let Ok(name) = HeaderName::from_str(name.as_str()) {
                   if let Ok(value) = HeaderValue::from_str(value.to_str().unwrap_or_default()) {
                       headers.insert(name, value);
                   }
               }
           }
    }
    request_builder = request_builder.headers(headers);
    
    // Add body if not empty
    if !body.is_empty() {
        request_builder = request_builder.body(body.to_vec());
    }
    
    // Send the request
    match request_builder.send().await {
        Ok(response) => {
            let status = StatusCode::from_u16(response.status().as_u16())
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            let mut client_resp = HttpResponse::build(status);
            
            // Copy response headers
            for (name, value) in response.headers() {
                if name.as_str().to_lowercase() != "connection" && 
                   name.as_str().to_lowercase() != "transfer-encoding" && 
                   name.as_str().to_lowercase() != "content-length" {
                    if let Ok(header_name) = actix_web::http::header::HeaderName::from_str(name.as_str()) {
                        if let Ok(header_value) = actix_web::http::header::HeaderValue::from_bytes(value.as_bytes()) {
                            client_resp.insert_header((header_name, header_value));
                        }
                    }
                }
            }
            
            // Get response body
            let bytes = response.bytes().await
                .map_err(|e| actix_web::error::ErrorInternalServerError(e))?;
            
            Ok(client_resp.body(bytes))
        },
        Err(err) => {
            error!("Request failed: {}", err);
            Ok(HttpResponse::build(StatusCode::BAD_GATEWAY)
                .body(format!("Failed to proxy request: {}", err)))
        }
    }
}

// Streaming version for when streaming is needed
async fn proxy_stream_handler(
    req: HttpRequest,
    body: web::Bytes,
    client: web::Data<Client>,
    config: web::Data<ProxyConfig>,
) -> Result<HttpResponse, Error> {
    let path = req.uri().path_and_query()
        .map(|p| p.as_str())
        .unwrap_or("");
    
    // Build the upstream URL
    let upstream_url = format!("{}{}", config.upstream_url, path);
    info!("Proxying streaming request to: {}", upstream_url);
    
    // Create forwarded request with same method
    let mut request_builder = client.request(
        reqwest::Method::from_str(req.method().as_str())
            .map_err(|e| actix_web::error::ErrorBadRequest(e))?,
        &upstream_url
    );
    
    // Copy headers from original request
    let mut headers = HeaderMap::new();
    for (name, value) in req.headers() {
        if name.as_str().to_lowercase() != "host" && 
           name.as_str().to_lowercase() != "connection" {
               if let Ok(name) = HeaderName::from_str(name.as_str()) {
                   if let Ok(value) = HeaderValue::from_str(value.to_str().unwrap_or_default()) {
                       headers.insert(name, value);
                   }
               }
           }
    }
    request_builder = request_builder.headers(headers);
    
    // Add body if not empty
    if !body.is_empty() {
        request_builder = request_builder.body(body.to_vec());
    }
    
    // Send the request
    match request_builder.send().await {
        Ok(response) => {
            let status = StatusCode::from_u16(response.status().as_u16())
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            let mut client_resp = HttpResponse::build(status);
            
            // Copy response headers
            for (name, value) in response.headers() {
                if name.as_str().to_lowercase() != "connection" && 
                   name.as_str().to_lowercase() != "transfer-encoding" && 
                   name.as_str().to_lowercase() != "content-length" {
                    if let Ok(header_name) = actix_web::http::header::HeaderName::from_str(name.as_str()) {
                        if let Ok(header_value) = actix_web::http::header::HeaderValue::from_bytes(value.as_bytes()) {
                            client_resp.insert_header((header_name, header_value));
                        }
                    }
                }
            }
            
            // Stream response body
            let stream = response
                .bytes_stream()
                .map_err(|e| actix_web::error::ErrorInternalServerError(e));
            
            Ok(client_resp.streaming(stream))
        },
        Err(err) => {
            error!("Request failed: {}", err);
            Ok(HttpResponse::build(StatusCode::BAD_GATEWAY)
                .body(format!("Failed to proxy request: {}", err)))
        }
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize logger
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));
    
    // Load config from environment
    let config = web::Data::new(ProxyConfig::from_env());
    info!("Configured upstream URL: {}", config.upstream_url);
    
    // Create HTTP client with TLS configuration
    let client = web::Data::new(create_client(&config));
    
    // Server configuration
    let server_host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let server_port = env::var("PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse::<u16>()
        .expect("PORT must be a valid number");
    
    // Get max payload size from environment or use a larger default (100MB)
    let max_payload_size = env::var("MAX_PAYLOAD_SIZE")
        .unwrap_or_else(|_| "104857600".to_string()) // 100MB in bytes
        .parse::<usize>()
        .expect("MAX_PAYLOAD_SIZE must be a valid number");
    
    // Get worker count from environment or use num_cpus
    let worker_count = env::var("WORKER_COUNT")
        .ok()
        .and_then(|count| count.parse::<usize>().ok())
        .unwrap_or_else(|| num_cpus::get());
    
    info!("Starting reverse proxy server on {}:{} with max payload size: {}MB and {} worker threads", 
        server_host, server_port, max_payload_size / 1048576, worker_count);
    
    // Create the HTTP server with increased payload size
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::clone(&config))
            .app_data(web::Data::clone(&client))
            // Set payload size limit
            .app_data(web::PayloadConfig::new(max_payload_size))
            .route("/stream/{tail:.*}", web::to(proxy_stream_handler))
            .default_service(web::to(proxy_handler))
    })
    .bind(format!("{}:{}", server_host, server_port))?
    .workers(worker_count)  // Use configured number of workers
    .run()
    .await
}