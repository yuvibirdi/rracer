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
use rust_fsm::StateMachineImpl;
use shared::{
    fsm::{RracerEvent, RracerState},
    passages::get_random_passage,
    protocol::{ClientMsg, ServerMsg},
    wpm::{accuracy, gross_wpm, net_wpm},
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
use tower_http::{cors::CorsLayer, services::{ServeDir, ServeFile}};
use tracing::{info, warn};
use uuid::Uuid;
use rand::Rng;

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
    keystroke_count: usize,
    is_bot: bool,
    bot_speed_wpm: Option<f64>,
}

struct Room {
    id: String,
    state: Arc<RwLock<RracerState>>,
    players: Arc<RwLock<HashMap<String, Player>>>,
    passage: Arc<RwLock<Option<String>>>,
    countdown_start: Arc<RwLock<Option<u64>>>,
    waiting_start: Arc<RwLock<Option<u64>>>,
    last_timer_second: std::sync::atomic::AtomicU64,
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
            waiting_start: Arc::new(RwLock::new(None)),
            last_timer_second: std::sync::atomic::AtomicU64::new(0),
            tx,
        }
    }

    async fn try_start_countdown(&self) {
        // Guard: only from Waiting and when >=2 humans
        let mut state = self.state.write().await;
        if *state != RracerState::Waiting { return; }
        let mut players = self.players.write().await;
        let human_count = players.values().filter(|p| !p.is_bot).count();
        if human_count < 2 { return; }

        if let Some(new_state) = RracerState::transition(&*state, &RracerEvent::Join) {
            *state = new_state;
            *self.countdown_start.write().await = Some(current_timestamp());
            *self.passage.write().await = Some(get_random_passage().to_string());

            // Seed bots to reach at least 5 players total
            let total_now = players.len();
            let needed = 5usize.saturating_sub(total_now);
            for i in 0..needed {
                let mut rng = rand::thread_rng();
                let wpm: f64 = rng.gen_range(40.0..90.0);
                let bot_id = format!("bot-{}-{}-{}", self.id, i, Uuid::new_v4());
                let bot_name = format!("Bot {}", i + 1);
                let bot = Player {
                    id: bot_id.clone(),
                    name: bot_name,
                    position: 0,
                    start_time: None,
                    last_keystroke: 0,
                    errors: 0,
                    finished: false,
                    keystroke_count: 0,
                    is_bot: true,
                    bot_speed_wpm: Some(wpm),
                };
                players.insert(bot_id, bot);
            }

            drop(players); // release before broadcasts
            self.broadcast_lobby().await;
            let _ = self.tx.send(ServerMsg::StateChange { state: "countdown".to_string() });
            if let Some(passage) = self.passage.read().await.as_ref() {
                let _ = self.tx.send(ServerMsg::Countdown { passage: passage.clone() });
            }
            info!("Room {} starting countdown with >=2 humans", self.id);
        }
    }

    async fn add_player(&self, player: Player) {
        info!("Adding player {} to room {}", player.name, self.id);
        let mut players = self.players.write().await;
        players.insert(player.id.clone(), player);

        info!("Room {} now has {} players", self.id, players.len());

        // Check if we should start waiting or countdown
        if players.len() >= 1 {
            let mut state = self.state.write().await;
            
            // If the game is finished, reset it to waiting state when new players join
            if *state == RracerState::Finished {
                info!("Resetting finished game for new player in room {}", self.id);
                *state = RracerState::Waiting;
                *self.passage.write().await = None;
                *self.countdown_start.write().await = None;
                *self.waiting_start.write().await = None;
                self.last_timer_second.store(0, std::sync::atomic::Ordering::Relaxed);
                
                // Reset all existing players
                for player in players.values_mut() {
                    player.position = 0;
                    player.start_time = None;
                    player.errors = 0;
                    player.finished = false;
                    player.keystroke_count = 0;
                }
            }
            
            if *state == RracerState::Waiting {
                // Defer to shared starter (releases locks internally)
                drop(players);
                drop(state);
                self.try_start_countdown().await;
            }
        }

    // Release locks before broadcasting lobby
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

        info!(
            "Broadcasting lobby update for room {}: {:?}",
            self.id, player_names
        );
        let _ = self.tx.send(ServerMsg::Lobby {
            players: player_names,
        });
    }

    async fn handle_keystroke(&self, player_id: &str, ch: char, ts: u64) {
        let mut players = self.players.write().await;
        let passage = self.passage.read().await;

        if let (Some(player), Some(passage_text)) = (players.get_mut(player_id), passage.as_ref()) {
            let current_state = *self.state.read().await;

            if current_state != RracerState::Racing {
                return;
            }

            if player.is_bot { return; }

            // Basic rate limiting: prevent extreme spam (allow up to 50 keystrokes per second)
            if ts - player.last_keystroke < 20 {
                // 20ms = 50 keystrokes per second max
                return; // Just ignore spam
            }
            player.last_keystroke = ts;
            player.keystroke_count += 1;

            // Additional anti-cheat: check for impossible typing speeds
            if let Some(start) = player.start_time {
                let elapsed_seconds = (ts - start) as f64 / 1000.0;
                if elapsed_seconds > 0.1 {
                    // Only check after 100ms
                    let current_wpm = gross_wpm(player.position, elapsed_seconds);
                    if current_wpm > 300.0 {
                        // Impossible speed threshold
                        warn!(
                            "Suspicious typing speed from player {}: {} WPM",
                            player_id, current_wpm
                        );
                        let _ = self.tx.send(ServerMsg::Error {
                            message: "Suspicious typing speed detected".to_string(),
                        });
                        return;
                    }
                }
            }

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
                id: player.name.clone(),
                            wpm,
                            accuracy: acc,
                        });
                    } else {
                        let _ = self.tx.send(ServerMsg::Progress {
                id: player.name.clone(),
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
            RracerState::Waiting => {
                // No auto-start timer. Waiting state handled in add_player/remove_player.
            }
            RracerState::Countdown => {
                if let Some(start_time) = *self.countdown_start.read().await {
                    let elapsed = current_timestamp() - start_time;
                    if elapsed >= 3000 {
                        // 3 second countdown
                        let mut state = self.state.write().await;
                        if let Some(new_state) =
                            RracerState::transition(&*state, &RracerEvent::CountdownElapsed)
                        {
                            *state = new_state;

                            if let Some(passage) = self.passage.read().await.as_ref() {
                                let _ = self.tx.send(ServerMsg::Start {
                                    passage: passage.clone(),
                                    t0: current_timestamp(),
                                });
                            }

                            // Start bot simulation tasks
                            self.start_bots().await;

                            info!("Room {} started racing", self.id);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    async fn update_player_progress(&self, player_id: &str, position: usize) {
        let mut players = self.players.write().await;
        if let Some(player) = players.get_mut(player_id) {
            player.position = position;

            let _ = self.tx.send(ServerMsg::Progress {
            id: player.name.clone(),
                pos: position,
            });
        }
    }

    async fn handle_player_finish(&self, player_id: &str, wpm: f64, accuracy: f64) {
        let mut players = self.players.write().await;
        if let Some(player) = players.get_mut(player_id) {
            player.finished = true;

            let _ = self.tx.send(ServerMsg::Finish {
                id: player.name.clone(),
                wpm,
                accuracy,
            });

            // Check if all players finished
            let all_finished = players.values().all(|p| p.finished);
            if all_finished && !players.is_empty() {
                drop(players); // Release lock before state transition
                let mut state = self.state.write().await;
                if let Some(new_state) = RracerState::transition(&*state, &RracerEvent::AllDone) {
                    *state = new_state;
                    let _ = self.tx.send(ServerMsg::StateChange {
                        state: "finished".to_string(),
                    });
                }
            }
        }
    }

    async fn start_bots(&self) {
        let room_id = self.id.clone();
        let passage_opt = self.passage.read().await.clone();
    let tx = self.tx.clone();
        let players_arc = self.players.clone();
    let state_arc = self.state.clone();
        if let Some(passage) = passage_opt {
            let len = passage.len();
            // snapshot of (id, name, speed) for bots to avoid holding locks in tasks
            let snapshot: Vec<(String, String, f64)> = {
                let guard = players_arc.read().await;
                guard
                    .iter()
                    .filter_map(|(id, p)| if p.is_bot { Some((id.clone(), p.name.clone(), p.bot_speed_wpm.unwrap_or(60.0))) } else { None })
                    .collect()
            };
            for (bot_id, name, speed) in snapshot.into_iter() {
                let tx_clone = tx.clone();
                let players_arc_clone = players_arc.clone();
                let state_arc_clone = state_arc.clone();
                // Calculate chars per second
                let cps = speed * 5.0 / 60.0;
                tokio::spawn(async move {
                    let mut pos: f64 = 0.0;
                    let mut last = current_timestamp();
                    let tick = Duration::from_millis(100);
                    loop {
                        tokio::time::sleep(tick).await;
                        let now = current_timestamp();
                        let dt = (now - last) as f64 / 1000.0;
                        last = now;
                        pos += cps * dt;
                        let mut ipos = pos.floor() as usize;
                        if ipos > len { ipos = len; }
                        let _ = tx_clone.send(ServerMsg::Progress { id: name.clone(), pos: ipos });
                        if ipos >= len {
                            let wpm = speed; // approx
                            let acc = 100.0;
                            let _ = tx_clone.send(ServerMsg::Finish { id: name.clone(), wpm, accuracy: acc });
                            // Mark bot finished in room state and check if all finished
                            {
                                let mut guard = players_arc_clone.write().await;
                                if let Some(p) = guard.get_mut(&bot_id) {
                                    p.finished = true;
                                    p.position = len;
                                }
                                let all_finished = guard.values().all(|p| p.finished);
                                if all_finished && !guard.is_empty() {
                                    // Drop guard before broadcasting state change? we only hold players lock
                                }
                            }
                            break;
                        }
                    }
                    // After loop, check again and broadcast finished state if everyone is done
                    let done = {
                        let guard = players_arc_clone.read().await;
                        guard.values().all(|p| p.finished) && !guard.is_empty()
                    };
                    if done {
                        // Transition to Finished state if possible
                        if let Ok(mut state) = state_arc_clone.try_write() {
                            if let Some(new_state) = RracerState::transition(&*state, &RracerEvent::AllDone) {
                                *state = new_state;
                                let _ = tx_clone.send(ServerMsg::StateChange { state: "finished".to_string() });
                            }
                        } else {
                            let _ = tx_clone.send(ServerMsg::StateChange { state: "finished".to_string() });
                        }
                    }
                });
            }
        } else {
            warn!("start_bots called with no passage for room {}", room_id);
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
    tracing_subscriber::fmt::init();

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
        // Serve WASM dist with SPA fallback; assumes `web/dist` built via Trunk
        .nest_service("/", ServeDir::new("web/dist").fallback(ServeFile::new("web/dist/index.html")))
        .layer(CorsLayer::permissive())
        .with_state(rooms);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    info!("Server running on http://0.0.0.0:3000");

    axum::serve(listener, app).await?;
    Ok(())
}

async fn ws_handler(ws: WebSocketUpgrade, State(rooms): State<Rooms>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, rooms))
}

async fn handle_socket(socket: WebSocket, rooms: Rooms) {
    let (mut sender, mut receiver) = socket.split();
    let player_id = Uuid::new_v4().to_string();
    let mut current_room: Option<String> = None;
    let mut _player_name: Option<String> = None;
    let mut room_rx: Option<broadcast::Receiver<ServerMsg>> = None;

    info!("New WebSocket connection established for player {}", player_id);

    loop {
        tokio::select! {
            // Incoming client messages
            ws_msg = receiver.next() => {
                match ws_msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(client_msg) = serde_json::from_str::<ClientMsg>(&text) {
                            match client_msg {
                                ClientMsg::Join { room, name } => {
                                    // Leave previous room
                                    if let Some(room_id) = &current_room {
                                        if let Some(room) = rooms.get(room_id) {
                                            room.remove_player(&player_id).await;
                                        }
                                    }

                                    // Join the new room
                                    let room = rooms.entry(room.clone()).or_insert_with(|| Room::new(room.clone()));
                                    room_rx = Some(room.tx.subscribe());

                                    let player = Player {
                                        id: player_id.clone(),
                                        name: name.clone(),
                                        position: 0,
                                        start_time: None,
                                        last_keystroke: 0,
                                        errors: 0,
                                        finished: false,
                                        keystroke_count: 0,
                                        is_bot: false,
                                        bot_speed_wpm: None,
                                    };

                                    room.add_player(player).await;
                                    current_room = Some(room.id.clone());
                                    _player_name = Some(name);
                                }
                                ClientMsg::Key { ch, ts } => {
                                    if let Some(room_id) = &current_room {
                                        if let Some(room) = rooms.get(room_id) {
                                            room.handle_keystroke(&player_id, ch, ts).await;
                                        }
                                    }
                                }
                                ClientMsg::Progress { pos, ts: _ } => {
                                    if let Some(room_id) = &current_room {
                                        if let Some(room) = rooms.get(room_id) {
                                            room.update_player_progress(&player_id, pos).await;
                                        }
                                    }
                                }
                                ClientMsg::Finish { wpm, accuracy, time: _, ts: _ } => {
                                    if let Some(room_id) = &current_room {
                                        if let Some(room) = rooms.get(room_id) {
                                            room.handle_player_finish(&player_id, wpm, accuracy).await;
                                        }
                                    }
                                }
                                ClientMsg::Reset => {
                                    if let Some(room_id) = &current_room {
                                        if let Some(room) = rooms.get(room_id) {
                                            // Transition to waiting and clear race state
                                            if let Some(new_state) = {
                                                let state = room.state.read().await.clone();
                                                RracerState::transition(&state, &RracerEvent::Reset)
                                            } {
                                                let mut state = room.state.write().await;
                                                *state = new_state;
                                            }
                                            *room.passage.write().await = None;
                                            *room.countdown_start.write().await = None;
                                            *room.waiting_start.write().await = None;
                                            room.last_timer_second.store(0, std::sync::atomic::Ordering::Relaxed);

                                            // Remove bots and reset humans
                                            let mut players = room.players.write().await;
                                            players.retain(|_, p| !p.is_bot);
                                            for player in players.values_mut() {
                                                player.position = 0;
                                                player.start_time = None;
                                                player.errors = 0;
                                                player.finished = false;
                                                player.keystroke_count = 0;
                                            }
                                            drop(players);

                                            let _ = room.tx.send(ServerMsg::StateChange { state: "waiting".to_string() });
                                            room.broadcast_lobby().await;
                                            room.try_start_countdown().await;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                    _ => {}
                }
            }

            // Room broadcast messages
            room_msg = async {
                if let Some(ref mut rx) = room_rx {
                    rx.recv().await
                } else {
                    std::future::pending().await
                }
            } => {
                match room_msg {
                    Ok(msg) => {
                        if let Ok(text) = serde_json::to_string(&msg) {
                            if sender.send(Message::Text(text)).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                }
            }
        }
    }

    // Cleanup
    if let Some(room_id) = &current_room {
        if let Some(room) = rooms.get(room_id) {
            room.remove_player(&player_id).await;
        }
    }
}
