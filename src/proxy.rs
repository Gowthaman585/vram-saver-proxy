// ============================================================================
// MODULE: Transparent Proxy Pipe (proxy.rs)
// Intercepts incoming HTTP requests, logs conversation history to SQLite,
// applies prompt truncation/token budgeting, forwards requests to Ollama,
// and captures assistant responses.
// ============================================================================
use axum::{
    body::Body,
    extract::Request,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};

// Structure to match incoming AI chat payloads (Ollama / OpenAI format)
#[derive(Deserialize, Serialize, Debug)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize, Serialize, Debug)]
struct ChatRequest {
    messages: Vec<ChatMessage>,
}

// Struct to capture the assistant's response from Ollama
#[derive(Deserialize, Debug)]
struct ChatResponse {
    message: ChatMessage,
}

/// Main handler for all proxied traffic. It reads the incoming request,
/// logs messages, truncates context for VRAM efficiency, and forwards to Ollama.
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

    let mut modified_body_bytes = body_bytes.clone();
    
    // 🔍 Single-pass parsing, logging, and VRAM budgeting
    if let Ok(mut chat_req) = serde_json::from_slice::<ChatRequest>(&body_bytes) {
        // Log all incoming messages to SQLite (full history preserved!)
        if let Ok(conn) = crate::db::init_db() {
            for msg in &chat_req.messages {
                println!("💾 Logging message to DB [{}]: {}", msg.role, msg.content);
                let _ = crate::db::insert_message(&conn, &msg.role, &msg.content);
            }
        }

        // ✂️ BUDGETING / TRUNCATION LOGIC WITH CONFIGURABLE EVN LIMIT
        let max_total_messages: usize = std::env::var("MAX_CONTEXT_MESSAGES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(6);
        if chat_req.messages.len() > max_total_messages {
            // Check if the very first message is a system prompt
            let has_system = chat_req.messages.first().map_or(false, |m| m.role == "system");
            
            if has_system {
                println!("🛡️ System prompt detected! Preserving it in the context window.");
                let system_msg = chat_req.messages.remove(0); // Safely take out the system prompt
                
                // Allow the remaining history to fit the rest of the budget
                let max_history = max_total_messages - 1;
                if chat_req.messages.len() > max_history {
                    let split_index = chat_req.messages.len() - max_history;
                    chat_req.messages = chat_req.messages.split_off(split_index);
                }
                
                // Put the system prompt back securely at index 0
                chat_req.messages.insert(0, system_msg);
            } else {
                // Standard truncation if no system prompt exists
                println!(
                    "✂️ Truncating context from {} to last {} messages to save VRAM",
                    chat_req.messages.len(),
                    max_total_messages
                );
                let split_index = chat_req.messages.len() - max_total_messages;
                chat_req.messages = chat_req.messages.split_off(split_index);
            }

            // Re-serialize the modified request back into JSON bytes for Ollama
            if let Ok(new_bytes) = serde_json::to_vec(&chat_req) {
                modified_body_bytes = axum::body::Bytes::from(new_bytes);
            }
        }
    }

    // Build the outgoing request to Ollama using `modified_body_bytes`
    let mut forward_req = client.request(method, &backend_url);
    for (key, value) in headers.iter() {
        forward_req = forward_req.header(key, value);
    }
    forward_req = forward_req.body(modified_body_bytes);

    // Send the request and handle the backend's response
    match forward_req.send().await {
        Ok(resp) => {
            let status = resp.status();
            let resp_headers = resp.headers().clone();
            
            let resp_bytes = match resp.bytes().await {
                Ok(bytes) => bytes,
                Err(_) => return Response::builder().status(500).body(Body::from("Failed to read backend response")).unwrap(),
            };

            // 🔍 Log incoming ASSISTANT response to DB!
            if let Ok(chat_resp) = serde_json::from_slice::<ChatResponse>(&resp_bytes) {
                if let Ok(conn) = crate::db::init_db() {
                    println!("💾 Logging model response to DB: {}", chat_resp.message.content);
                    let _ = crate::db::insert_message(&conn, &chat_resp.message.role, &chat_resp.message.content);
                }
            }

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
