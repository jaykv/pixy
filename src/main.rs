use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer, Error};
use actix_web::http::StatusCode;
use futures::TryStreamExt;
use log::{info, error};
use env_logger;
use reqwest::{Client, ClientBuilder};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::str::FromStr;
use clap::{App as ClapApp, Arg};

// Configuration struct
struct ProxyConfig {
    upstream_url: String,
    client_cert_path: Option<String>,
    client_key_path: Option<String>,
    ca_cert_path: Option<String>,
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
    // [proxy_handler implementation remains unchanged]
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
    // [proxy_stream_handler implementation remains unchanged]
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
    
    // Parse command line arguments
    let matches = ClapApp::new("pixy")
        .version("1.0.0")
        .author("")
        .about("A reverse proxy server with TLS support")
        .arg(Arg::with_name("upstream_url")
            .long("upstream-url")
            .value_name("URL")
            .help("Upstream server URL to proxy requests to")
            .takes_value(true)
            .required(true))
        .arg(Arg::with_name("client_cert_path")
            .long("client-cert")
            .value_name("FILE")
            .help("Path to client certificate PEM file")
            .takes_value(true))
        .arg(Arg::with_name("client_key_path")
            .long("client-key")
            .value_name("FILE")
            .help("Path to client key PEM file")
            .takes_value(true))
        .arg(Arg::with_name("ca_cert_path")
            .long("ca-cert")
            .value_name("FILE")
            .help("Path to CA certificate PEM file")
            .takes_value(true))
        .arg(Arg::with_name("host")
            .long("host")
            .value_name("ADDRESS")
            .help("Host address to bind the server to")
            .takes_value(true)
            .default_value("0.0.0.0"))
        .arg(Arg::with_name("port")
            .long("port")
            .value_name("PORT")
            .help("Port to bind the server to")
            .takes_value(true)
            .default_value("8080"))
        .arg(Arg::with_name("worker_count")
            .long("workers")
            .value_name("COUNT")
            .help("Number of worker threads (default: number of CPU cores)")
            .takes_value(true))
        .arg(Arg::with_name("max_payload_size")
            .long("max-payload")
            .value_name("BYTES")
            .help("Maximum payload size in bytes")
            .takes_value(true)
            .default_value("104857600"))  // 100MB default
        .get_matches();
    
    // Load config from arguments
    let config = web::Data::new(ProxyConfig {
        upstream_url: matches.value_of("upstream_url").unwrap().to_string(),
        client_cert_path: matches.value_of("client_cert_path").map(String::from),
        client_key_path: matches.value_of("client_key_path").map(String::from),
        ca_cert_path: matches.value_of("ca_cert_path").map(String::from),
    });
    
    info!("Configured upstream URL: {}", config.upstream_url);
    
    // Create HTTP client with TLS configuration
    let client = web::Data::new(create_client(&config));
    
    // Server configuration
    let server_host = matches.value_of("host").unwrap();
    let server_port = matches.value_of("port").unwrap()
        .parse::<u16>()
        .expect("PORT must be a valid number");
    
    // Get max payload size
    let max_payload_size = matches.value_of("max_payload_size").unwrap()
        .parse::<usize>()
        .expect("MAX_PAYLOAD_SIZE must be a valid number");
    
    // Get worker count or use num_cpus
    let worker_count = matches.value_of("worker_count")
        .map(|count| count.parse::<usize>().expect("WORKER_COUNT must be a valid number"))
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
    .workers(worker_count)
    .run()
    .await
}