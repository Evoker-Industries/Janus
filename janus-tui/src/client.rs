//! WebSocket client for management API

use anyhow::Result;
use futures::{stream::SplitSink, SinkExt, StreamExt};
use janus_common::{ClientMessage, ServerMessage};
use std::collections::VecDeque;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};
use tracing::{debug, error};

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;
type WsSink = SplitSink<WsStream, Message>;

/// Management API client
pub struct ManagementClient {
    /// Channel to send messages to the WebSocket task
    tx: mpsc::Sender<ClientMessage>,
    
    /// Buffer of received messages
    received: VecDeque<ServerMessage>,
    
    /// Channel to receive messages from the WebSocket task
    rx: mpsc::Receiver<ServerMessage>,
}

impl ManagementClient {
    /// Connect to the management server
    pub async fn connect(url: &str) -> Result<Self> {
        let (ws_stream, _) = connect_async(url).await?;
        let (write, read) = ws_stream.split();

        // Create channels
        let (tx, mut cmd_rx) = mpsc::channel::<ClientMessage>(100);
        let (msg_tx, rx) = mpsc::channel::<ServerMessage>(100);

        // Spawn task to handle WebSocket communication
        tokio::spawn(async move {
            if let Err(e) = run_client(write, read, &mut cmd_rx, &msg_tx).await {
                error!("WebSocket client error: {}", e);
            }
        });

        Ok(Self {
            tx,
            received: VecDeque::new(),
            rx,
        })
    }

    /// Send a message to the server
    pub async fn send(&mut self, msg: ClientMessage) -> Result<()> {
        self.tx.send(msg).await?;
        Ok(())
    }

    /// Try to receive a message (non-blocking)
    pub fn try_recv(&mut self) -> Option<ServerMessage> {
        // First check buffered messages
        if let Some(msg) = self.received.pop_front() {
            return Some(msg);
        }
        
        // Try to receive from channel
        match self.rx.try_recv() {
            Ok(msg) => Some(msg),
            Err(_) => None,
        }
    }
}

/// Run the WebSocket client
async fn run_client(
    mut write: WsSink,
    mut read: futures::stream::SplitStream<WsStream>,
    cmd_rx: &mut mpsc::Receiver<ClientMessage>,
    msg_tx: &mpsc::Sender<ServerMessage>,
) -> Result<()> {
    loop {
        tokio::select! {
            // Handle outgoing messages
            Some(cmd) = cmd_rx.recv() => {
                let text = serde_json::to_string(&cmd)?;
                debug!("Sending: {}", text);
                write.send(Message::Text(text)).await?;
            }
            
            // Handle incoming messages
            Some(msg) = read.next() => {
                match msg {
                    Ok(Message::Text(text)) => {
                        debug!("Received: {}", text);
                        match serde_json::from_str::<ServerMessage>(&text) {
                            Ok(server_msg) => {
                                if msg_tx.send(server_msg).await.is_err() {
                                    break;
                                }
                            }
                            Err(e) => {
                                error!("Failed to parse message: {}", e);
                            }
                        }
                    }
                    Ok(Message::Close(_)) => {
                        debug!("Server closed connection");
                        break;
                    }
                    Ok(Message::Ping(data)) => {
                        write.send(Message::Pong(data)).await?;
                    }
                    Ok(_) => {}
                    Err(e) => {
                        error!("WebSocket error: {}", e);
                        break;
                    }
                }
            }
            
            else => break,
        }
    }
    
    Ok(())
}
