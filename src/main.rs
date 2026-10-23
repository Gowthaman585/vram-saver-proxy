use rusqlite::{Connection, Result};

fn init_db() -> Result<Connection> {
    // 1. Open (or create) a local SQLite database file named 'local_memory.db'
    let conn = Connection::open("chat_history.db")?;

    // 2. Create a table to store our conversation history
    conn.execute(
        "CREATE TABLE IF NOT EXISTS messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
        )",
        [],
    )?;     //?: allows the below code to execute only when table is created or else exit

    Ok(conn)        //returns the success state
}

fn main() {
    println!("📦 Initializing local database...");

    match init_db() {
        // _conn is used to get rid of the warinings of unused varaible of conn
        Ok(_conn) => println!("✅ Database initialized and table created successfully!"),
        Err(e) => eprintln!("❌ Failed to initialize database: {}", e),
    }
}
