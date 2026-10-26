// ============================================================================
// MODULE: Transparent Proxy Pipe (proxy.rs)
// Intercepts incoming HTTP requests, forwards them to the local LLM backend
// (Ollama), and pipes the response back to the client.
// ============================================================================

use axum::{
    body::Body,
    extract::Request,
    response::{IntoResponse, Response},
};

/// Main handler for all proxied traffic. It reads the incoming request,
/// replicates its method, headers, and body, and sends it to Ollama.
pub async fn proxy_handler(req: Request) -> impl IntoResponse {
    let client = reqwest::Client::new();
    
    // Target backend: Ollama's default local server address
    let backend_url = format!("http://127.0.0.1:11434{}", req.uri().path());

    // Extract request components to reconstruct for the backend
    let method = req.method().clone();
    let headers = req.headers().clone();
    
    // Read the incoming request body into raw bytes
    let body_bytes = match axum::body::to_bytes(req.into_body(), usize::MAX).await {
        Ok(bytes) => bytes,
        Err(_) => return Response::builder().status(500).body(Body::from("Failed to read request body")).unwrap(),
    };

    // Build the outgoing request to Ollama
    let mut forward_req = client.request(method, &backend_url);
    for (key, value) in headers.iter() {
        forward_req = forward_req.header(key, value);
    }
    forward_req = forward_req.body(body_bytes);

    // Send the request and handle the backend's response
    match forward_req.send().await {
        Ok(resp) => {
            let status = resp.status();
            let resp_headers = resp.headers().clone();
            
            let resp_bytes = match resp.bytes().await {
                Ok(bytes) => bytes,
                Err(_) => return Response::builder().status(500).body(Body::from("Failed to read backend response")).unwrap(),
            };

            // Reconstruct the response to send back to the user's tool
            let mut response_builder = Response::builder().status(status);
            for (key, value) in resp_headers.iter() {
                response_builder = response_builder.header(key, value);
            }

            response_builder.body(Body::from(resp_bytes)).unwrap()
        }
        Err(e) => {
            eprintln!("❌ Proxy forwarding error: {}", e);
            Response::builder().status(502).body(Body::from(format!("Bad Gateway: {}", e))).unwrap()
        }
    }
}
