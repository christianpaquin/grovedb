use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use anyhow::anyhow;
use base64::{Engine as _, engine::general_purpose};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use tower_http::cors::CorsLayer;
use tracing::{error, info};

mod document;
use document::Document;

// Message types for WebSocket communication
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
enum ClientMessage {
    #[serde(rename = "insert")]
    Insert { 
        target_uuid: Option<String>,  // None = insert at beginning, Some = insert after this UUID
        uuid: String,  // Client-generated UUID for the new character
        value: char 
    },
    #[serde(rename = "delete")]
    Delete { uuid: String },  // Delete by UUID (reference-based)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
enum ServerMessage {
    #[serde(rename = "initial")]
    Initial {
        content: Vec<(String, char)>,
        root_hash: String,
    },
    #[serde(rename = "operation")]
    Operation {
        operation: String, // "insert" or "delete"
        target_uuid: Option<String>, // For insert: UUID inserted after (None = beginning)
        uuid: String,  // UUID of the character
        value: Option<char>,  // For insert operations
        root_hash: String,
        proof: String, // base64-encoded proof
    },
    #[serde(rename = "error")]
    Error { message: String },
}

// Application state
struct AppState {
    document: Arc<Mutex<Document>>,
    tx: broadcast::Sender<ServerMessage>,
    changelog_path: std::path::PathBuf,
    op_index: Arc<Mutex<u64>>, // Track operation index for changelog
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "merk_collab_server=debug,tower_http=debug".into()),
        )
        .init();

    info!("Starting Merk collaborative editing server");

    // Create temporary directory for Merk storage
    let temp_dir = tempfile::tempdir()?;
    info!("Using storage directory: {:?}", temp_dir.path());

    // Initialize document
    let document = Document::new(temp_dir.path())?;
    let document = Arc::new(Mutex::new(document));

    // Create broadcast channel for updates
    let (tx, _rx) = broadcast::channel(100);

    // Create changelog file path
    let changelog_path = temp_dir.path().join("changelog.jsonl");
    info!("========================================");
    info!("Changelog location: {}", changelog_path.display());
    info!("To audit, run: cargo run --manifest-path merk-collab-demo/auditor/Cargo.toml -- {}", changelog_path.display());
    info!("========================================");

    let state = Arc::new(AppState { 
        document, 
        tx,
        changelog_path,
        op_index: Arc::new(Mutex::new(0)),
    });

    // Build router
    let app = Router::new()
        .route("/", get(root))
        .route("/ws", get(websocket_handler))
        .route("/document", get(get_document))
        .route("/health", get(health))
        .layer(CorsLayer::permissive())
        .with_state(state);

    // Start server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;
    info!("Server listening on http://127.0.0.1:3000");

    axum::serve(listener, app).await?;

    Ok(())
}

async fn root() -> &'static str {
    "Merk Collaborative Editing Server"
}

async fn health() -> &'static str {
    "OK"
}

async fn get_document(State(state): State<Arc<AppState>>) -> Json<ServerMessage> {
    let doc = state.document.lock().await;
    let content = doc.get_content();
    let root_hash = doc.root_hash_hex();

    Json(ServerMessage::Initial { content, root_hash })
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();

    // Send initial document state
    {
        let doc = state.document.lock().await;
        let content = doc.get_content();
        let root_hash = doc.root_hash_hex();

        let initial_msg = ServerMessage::Initial { content, root_hash };
        if let Ok(json) = serde_json::to_string(&initial_msg) {
            if sender.send(Message::Text(json)).await.is_err() {
                error!("Failed to send initial state");
                return;
            }
        }
    }

    // Subscribe to broadcast updates
    let mut rx = state.tx.subscribe();

    // Spawn task to forward broadcast messages to this client
    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if let Ok(json) = serde_json::to_string(&msg) {
                if sender.send(Message::Text(json)).await.is_err() {
                    break;
                }
            }
        }
    });

    // Handle incoming messages from this client
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if let Message::Text(text) = msg {
                match serde_json::from_str::<ClientMessage>(&text) {
                    Ok(client_msg) => {
                        if let Err(e) = handle_client_message(client_msg, &state).await {
                            error!("Error handling client message: {}", e);
                        }
                    }
                    Err(e) => {
                        error!("Failed to parse client message: {}", e);
                    }
                }
            }
        }
    });

    // Wait for either task to finish
    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }
}

async fn handle_client_message(
    msg: ClientMessage,
    state: &Arc<AppState>,
) -> anyhow::Result<()> {
    // In a production system, verify the operation signature here:
    // let signature = msg.signature; // Would be included in ClientMessage
    // let user_id = msg.user_id;     // Would be included in ClientMessage
    // if !verify_signature(&user_id, &msg, &signature) {
    //     return Err(anyhow!("Invalid signature"));
    // }
    //
    // For this demo, we trust all operations and focus on Merk functionality.
    
    // Get next operation index
    let op_index = {
        let mut index = state.op_index.lock().await;
        let current = *index;
        *index += 1;
        current
    };
    
    let mut doc = state.document.lock().await;

    let server_msg = match msg {
        ClientMessage::Insert { target_uuid, uuid: uuid_str, value } => {
            // Parse the client-provided UUID
            let uuid = uuid::Uuid::parse_str(&uuid_str)
                .map_err(|e| anyhow!("Invalid UUID: {}", e))?;
            let uuid_bytes = uuid.as_bytes().to_vec();

            // Parse target UUID if provided
            let target_uuid_bytes = if let Some(target_str) = &target_uuid {
                let target_uuid = uuid::Uuid::parse_str(target_str)
                    .map_err(|e| anyhow!("Invalid target UUID: {}", e))?;
                Some(target_uuid.as_bytes().to_vec())
            } else {
                None
            };

            // Apply insert operation with reference-based approach
            let (actual_uuid, root_hash, proof) = doc.insert_after(target_uuid_bytes, uuid_bytes, value)?;

            // Encode proof as base64
            let proof_base64 = general_purpose::STANDARD.encode(&proof);

            // Convert UUID bytes to string
            let uuid_str = uuid::Uuid::from_bytes(
                actual_uuid.as_slice().try_into().unwrap()
            ).to_string();
            
            let root_hash_hex = hex::encode(root_hash);

            // Write to changelog for audit trail
            let changelog_entry = ChangelogEntry {
                op_index,
                operation: "insert".to_string(),
                target_uuid: target_uuid.clone(),
                uuid: uuid_str.clone(),
                value: Some(value),
                proof: proof_base64.clone(),
                new_root_hash: root_hash_hex.clone(),
            };
            if let Err(e) = append_to_changelog(&state.changelog_path, &changelog_entry) {
                error!("Failed to write to changelog: {}", e);
            } else {
                info!("Wrote insert operation to changelog (index: {})", op_index);
            }

            ServerMessage::Operation {
                operation: "insert".to_string(),
                target_uuid,
                uuid: uuid_str,
                value: Some(value),
                root_hash: root_hash_hex,
                proof: proof_base64,
            }
        }
        ClientMessage::Delete { uuid: uuid_str } => {
            // Parse the UUID
            let uuid = uuid::Uuid::parse_str(&uuid_str)
                .map_err(|e| anyhow!("Invalid UUID: {}", e))?;
            let uuid_bytes = uuid.as_bytes().to_vec();

            // Apply delete operation (reference-based: delete by UUID)
            let (deleted_uuid, root_hash, proof) = doc.delete_by_uuid(uuid_bytes)?;

            // Encode proof as base64
            let proof_base64 = general_purpose::STANDARD.encode(&proof);

            // Convert UUID bytes to string
            let uuid_str = uuid::Uuid::from_bytes(
                deleted_uuid.as_slice().try_into().unwrap()
            ).to_string();
            
            let root_hash_hex = hex::encode(root_hash);

            // Write to changelog for audit trail
            let changelog_entry = ChangelogEntry {
                op_index,
                operation: "delete".to_string(),
                target_uuid: None,
                uuid: uuid_str.clone(),
                value: None,
                proof: proof_base64.clone(),
                new_root_hash: root_hash_hex.clone(),
            };
            if let Err(e) = append_to_changelog(&state.changelog_path, &changelog_entry) {
                error!("Failed to write to changelog: {}", e);
            } else {
                info!("Wrote delete operation to changelog (index: {})", op_index);
            }

            ServerMessage::Operation {
                operation: "delete".to_string(),
                target_uuid: None,
                uuid: uuid_str,
                value: None,
                root_hash: root_hash_hex,
                proof: proof_base64,
            }
        }
    };

    // Broadcast to all connected clients
    info!("Broadcasting operation: {:?}", server_msg);
    let subscriber_count = state.tx.receiver_count();
    info!("Number of subscribers: {}", subscriber_count);
    let _ = state.tx.send(server_msg.clone());

    Ok(())
}

use futures_util::StreamExt;
use futures_util::stream::SplitStream;
use futures_util::stream::SplitSink;
use futures_util::SinkExt;
use std::io::Write;
use std::fs::OpenOptions;

// Changelog entry for audit trail
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChangelogEntry {
    op_index: u64,
    operation: String, // "insert" or "delete"
    target_uuid: Option<String>, // For insert: UUID inserted after (None = beginning)
    uuid: String, // UUID of the character being inserted/deleted
    value: Option<char>,
    proof: String, // base64-encoded Merkle proof
    new_root_hash: String,
}

// Append an entry to the changelog file
fn append_to_changelog(path: &std::path::Path, entry: &ChangelogEntry) -> anyhow::Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    
    let json = serde_json::to_string(entry)?;
    writeln!(file, "{}", json)?;
    
    Ok(())
}
