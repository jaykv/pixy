# pixy

A reverse proxy server with TLS support. 

**Note: Experimental!** Vibe coded with Claude.

## Usage

```sh
pixy --upstream-url <UPSTREAM_URL>
```

## Arguments

- `--upstream-url`: Upstream server URL to proxy requests to
- `--client-cert`: Path to client certificate PEM file
- `--client-key`: Path to client key PEM file
- `--ca-cert`: Path to CA certificate PEM file
- `--host`: Host address to bind the server to
- `--port`: Port to bind the server to
- `--workers`: Number of worker threads (default: number of CPU cores)
- `--max-payload`: Maximum payload size in bytes (default: 100MB)

## Example

```sh
pixy --upstream-url https://example.com --client-cert /path/to/client.pem --client-key /path/to/key.pem --ca-cert /path/to/ca.pem --host 0.0.0.0 --port 8080 --workers 4 --max-payload 104857600
```

## Why

Most LLM tools don't support mTLS out-of-the-box, so this is a simple way to work around it. 

Configure the LLM tool to connect to the proxy server and it will use the proxy to connect to the upstream server and relay the requests. 
