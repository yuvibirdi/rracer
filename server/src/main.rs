use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use dashmap::DashMap;
use futures::{sink::SinkExt, stream::StreamExt};
use shared::{
    fsm::{RracerEvent, RracerState},
    passages::get_random_passage,
    protocol::{ClientMsg, ServerMsg},
    wpm::{gross_wpm, net_wpm, accuracy},
};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{
    sync::{broadcast, RwLock},
    time::{interval, Duration},
};
use tower_http::{cors::CorsLayer, services::ServeDir};
use tracing::{error, info, warn};
use uuid::Uuid;

type Rooms = Arc<DashMap<String, Room>>;

#[derive(Clone)]
struct Player {
    id: String,
    name: String,
    position: usize,
    start_time: Option<u64>,
    last_keystroke: u64,
    errors: usize,
    finished: bool,
    tx: broadcast::Sender<ServerMsg>,
}

struct Room {
    id: String,
    state: Arc<RwLock<RracerState>>,
    players: Arc<RwLock<HashMap<String, Player>>>,
    passage: Arc<RwLock<Option<String>>>,
    countdown_start: Arc<RwLock<Option<u64>>>,
    tx: broadcast::Sender<ServerMsg>,
}

impl Room {
    fn new(id: String) -> Self {
        let (tx, _) = broadcast::channel(100);
        Self {
            id,
            state: Arc::new(RwLock::new(RracerState::Waiting)),
            players: Arc::new(RwLock::new(HashMap::new())),
            passage: Arc::new(RwLock::new(None)),
            countdown_start: Arc::new(RwLock::new(None)),
            tx,
        }
    }

    async fn add_player(&self, player: Player) {
        let mut players = self.players.write().await;
        players.insert(player.id.clone(), player);
        
        // Check if we should start countdown
        if players.len() >= 2 {
            let mut state = self.state.write().await;
            if *state == RracerState::Waiting {
                if let Some(new_state) = RracerState::transition(&*state, &RracerEvent::Join) {
                    *state = new_state;
                    *self.countdown_start.write().await = Some(current_timestamp());
                    *self.passage.write().await = Some(get_random_passage().to_string());
                    
                    let _ = self.tx.send(ServerMsg::StateChange {
                        state: "countdown".to_string(),
                    });
                    
                    info!("Room {} starting countdown", self.id);
                }
            }
        }
        
        self.broadcast_lobby().await;
    }

    async fn remove_player(&self, player_id: &str) {
        let mut players = self.players.write().await;
        players.remove(player_id);
        
        if players.is_empty() {
            let mut state = self.state.write().await;
            *state = RracerState::Waiting;
            *self.passage.write().await = None;
            *self.countdown_start.write().await = None;
        }
        
        self.broadcast_lobby().await;
    }

    async fn broadcast_lobby(&self) {
        let players = self.players.read().await;
        let player_names: Vec<String> = players.values().map(|p| p.name.clone()).collect();
        
        let _ = self.tx.send(ServerMsg::Lobby { players: player_names });
    }

    async fn handle_keystroke(&self, player_id: &str, ch: char, ts: u64) {
        let mut players = self.players.write().await;
        let passage = self.passage.read().await;
        
        if let (Some(player), Some(passage_text)) = (players.get_mut(player_id), passage.as_ref()) {
            let current_state = *self.state.read().await;
            
            if current_state != RracerState::Racing {
                return;
            }
            
            // Rate limiting: max 20 keystrokes per 100ms
            if ts - player.last_keystroke < 5 {
                warn!("Rate limiting player {}", player_id);
                return;
            }
            player.last_keystroke = ts;
            
            // Validate keystroke
            if let Some(expected_char) = passage_text.chars().nth(player.position) {
                if ch == expected_char {
                    player.position += 1;
                    
                    if player.start_time.is_none() {
                        player.start_time = Some(ts);
                    }
                    
                    // Check if player finished
                    if player.position >= passage_text.len() {
                        player.finished = true;
                        let elapsed = (ts - player.start_time.unwrap_or(ts)) as f64 / 1000.0;
                        let wpm = net_wpm(player.position, elapsed, player.errors);
                        let acc = accuracy(player.position - player.errors, player.position);
                        
                        let _ = self.tx.send(ServerMsg::Finish {
                            id: player_id.to_string(),
                            wpm,
                            accuracy: acc,
                        });
                    } else {
                        let _ = self.tx.send(ServerMsg::Progress {
                            id: player_id.to_string(),
                            pos: player.position,
                        });
                    }
                } else {
                    player.errors += 1;
                }
            }
        }
        
        // Check if all players finished
        let all_finished = players.values().all(|p| p.finished);
        if all_finished && !players.is_empty() {
            let mut state = self.state.write().await;
            if let Some(new_state) = RracerState::transition(&*state, &RracerEvent::AllDone) {
                *state = new_state;
                let _ = self.tx.send(ServerMsg::StateChange {
                    state: "finished".to_string(),
                });
            }
        }
    }

    async fn tick(&self) {
        let current_state = *self.state.read().await;
        
        match current_state {
            RracerState::Countdown => {
                if let Some(start_time) = *self.countdown_start.read().await {
                    let elapsed = current_timestamp() - start_time;
                    if elapsed >= 3000 { // 3 second countdown
                        let mut state = self.state.write().await;
                        if let Some(new_state) = RracerState::transition(&*state, &RracerEvent::CountdownElapsed) {
                            *state = new_state;
                            
                            if let Some(passage) = self.passage.read().await.as_ref() {
                                let _ = self.tx.send(ServerMsg::Start {
                                    passage: passage.clone(),
                                    t0: current_timestamp(),
                                });
                            }
                            
                            info!("Room {} started racing", self.id);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::init();
    
    let rooms: Rooms = Arc::new(DashMap::new());
    
    // Spawn tick loop
    let rooms_tick = rooms.clone();
    tokio::spawn(async move {
        let mut interval = interval(Duration::from_millis(50));
        loop {
            interval.tick().await;
            for room in rooms_tick.iter() {
                room.value().tick().await;
            }
        }
    });
    
    let app = Router::new()
        .route("/ws", get(ws_handler))
        .nest_service("/", ServeDir::new("../web/dist"))
        .layer(CorsLayer::permissive())
        .with_state(rooms);
    
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    info!("Server running on http://0.0.0.0:3000");
    
    axum::serve(listener, app).await?;
    Ok(())
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(rooms): State<Rooms>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, rooms))
}

async fn handle_socket(socket: WebSocket, rooms: Rooms) {
    let (mut sender, mut receiver) = socket.split();
    let player_id = Uuid::new_v4().to_string();
    let mut current_room: Option<String> = None;
    let mut player_name: Option<String> = None;
    
    // Create a channel for this connection
    let (tx, mut rx) = broadcast::channel::<ServerMsg>(100);
    
    // Spawn task to forward messages to client
    let player_id_clone = player_id.clone();
    tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if let Ok(text) = serde_json::to_string(&msg) {
                if sender.send(Message::Text(text)).await.is_err() {
                    break;
                }
            }
        }
        info!("Player {} disconnected (sender)", player_id_clone);
    });
    
    // Handle incoming messages
    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                if let Ok(client_msg) = serde_json::from_str::<ClientMsg>(&text) {
                    match client_msg {
                        ClientMsg::Join { room, name } => {
                            // Leave current room if any
                            if let Some(room_id) = &current_room {
                                if let Some(room) = rooms.get(room_id) {
                                    room.remove_player(&player_id).await;
                                }
                            }
                            
                            // Join new room
                            let room = rooms.entry(room.clone()).or_insert_with(|| Room::new(room.clone()));
                            let player = Player {
                                id: player_id.clone(),
                                name: name.clone(),
                                position: 0,
                                start_time: None,
                                last_keystroke: 0,
                                errors: 0,
                                finished: false,
                                tx: tx.clone(),
                            };
                            
                            room.add_player(player).await;
                            current_room = Some(room.id.clone());
                            player_name = Some(name);
                            
                            info!("Player {} joined room {}", player_id, room.id);
                        }
                        ClientMsg::Key { ch, ts } => {
                            if let Some(room_id) = &current_room {
                                if let Some(room) = rooms.get(room_id) {
                                    room.handle_keystroke(&player_id, ch, ts).await;
                                }
                            }
                        }
                        ClientMsg::Reset => {
                            if let Some(room_id) = &current_room {
                                if let Some(room) = rooms.get(room_id) {
                                    let mut state = room.state.write().await;
                                    if let Some(new_state) = RracerState::transition(&*state, &RracerEvent::Reset) {
                                        *state = new_state;
                                        *room.passage.write().await = None;
                                        *room.countdown_start.write().await = None;
                                        
                                        // Reset all players
                                        let mut players = room.players.write().await;
                                        for player in players.values_mut() {
                                            player.position = 0;
                                            player.start_time = None;
                                            player.errors = 0;
                                            player.finished = false;
                                        }
                                        
                                        let _ = room.tx.send(ServerMsg::StateChange {
                                            state: "waiting".to_string(),
                                        });
                                        
                                        room.broadcast_lobby().await;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }
    
    // Cleanup on disconnect
    if let Some(room_id) = &current_room {
        if let Some(room) = rooms.get(room_id) {
            room.remove_player(&player_id).await;
        }
    }
    
    info!("Player {} disconnected", player_id);
}
