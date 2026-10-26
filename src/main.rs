// ============================================================================
// MAIN APPLICATION ENTRY POINT (main.rs)
// Acts as the conductor: wires modules together, initializes services,
// configures the router, and launches the Axum web server.
// ============================================================================

mod db;
mod proxy;

use axum::{
    routing::get,
    Router,
};

/// Simple root endpoint to verify the server is live and responding.
async fn health_check() -> &'static str {
    "🤖 VRAM Saver Proxy is online and listening!"
}

#[tokio::main]
async fn main() {
    println!("📦 Initializing local database...");
    
    // Step 1: Initialize the database layer via our `db` module
    match db::init_db() {
        Ok(_conn) => println!("✅ Database initialized successfully!"),
        Err(e) => {
            eprintln!("❌ Failed to initialize database: {}", e);
            return;
        }
    }

    // Step 2: Configure the Axum web router
    // - Root ("/") handles local health checks
    // - Fallback handles all other paths, routing them through our proxy pipe
    let app = Router::new()
        .route("/", get(health_check))
        .fallback(proxy::proxy_handler);

    // Step 3: Bind the TCP listener to localhost port 3000
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    
    println!("🌐 Proxy server running at http://127.0.0.1:3000");

    // Step 4: Launch the asynchronous server using Tokio and Axum
    axum::serve(listener, app).await.unwrap();
}
