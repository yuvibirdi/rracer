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
use rand::Rng;
use rust_fsm::StateMachineImpl;
use shared::{
    fsm::{RracerEvent, RracerState},
    protocol::{ClientMsg, ServerMsg},
    wpm::{accuracy, gross_wpm, net_wpm},
};
use sqlx::PgPool;
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

mod db;
use db::get_random_passage as db_get_random_passage;

type Rooms = Arc<DashMap<String, Room>>;

#[derive(Clone)]
struct AppState {
    rooms: Rooms,
    db: Option<Arc<PgPool>>,
}

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
    db: Option<Arc<PgPool>>,
}

impl Room {
    fn new(id: String, db: Option<Arc<PgPool>>) -> Self {
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
            db,
        }
    }

    async fn try_start_countdown(&self) {
        let mut state = self.state.write().await;
        if *state != RracerState::Waiting { return; }
        let mut players = self.players.write().await;
        let human_count = players.values().filter(|p| !p.is_bot).count();
        if human_count < 2 { return; }

        if let Some(new_state) = RracerState::transition(&*state, &RracerEvent::Join) {
            *state = new_state;
            *self.countdown_start.write().await = Some(current_timestamp());
            let p = db_get_random_passage(self.db.as_deref()).await;
            *self.passage.write().await = Some(p);

            // Seed bots up to 5 total
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

            drop(players);
            self.broadcast_lobby().await;
            let _ = self.tx.send(ServerMsg::StateChange { state: "countdown".to_string() });
            if let Some(p) = self.passage.read().await.as_ref() { let _ = self.tx.send(ServerMsg::Countdown { passage: p.clone() }); }
            info!("Room {} starting countdown with >=2 humans", self.id);
        }
    }

    async fn add_player(&self, player: Player) {
        info!("Adding player {} to room {}", player.name, self.id);
        let mut players = self.players.write().await;
        players.insert(player.id.clone(), player);
        info!("Room {} now has {} players", self.id, players.len());

        if players.len() >= 1 {
            let mut state = self.state.write().await;
            if *state == RracerState::Finished {
                info!("Resetting finished game for new player in room {}", self.id);
                *state = RracerState::Waiting;
                *self.passage.write().await = None;
                *self.countdown_start.write().await = None;
                *self.waiting_start.write().await = None;
                self.last_timer_second.store(0, std::sync::atomic::Ordering::Relaxed);
                for p in players.values_mut() {
                    p.position = 0; p.start_time=None; p.errors=0; p.finished=false; p.keystroke_count=0;
                }
            }
        }
        drop(players);
        self.broadcast_lobby().await;
        self.try_start_countdown().await;
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
        let names: Vec<String> = players.values().map(|p| p.name.clone()).collect();
        info!("Broadcasting lobby update for room {}: {:?}", self.id, names);
        let _ = self.tx.send(ServerMsg::Lobby { players: names });
    }

    async fn handle_keystroke(&self, player_id: &str, ch: char, ts: u64) {
        let mut players = self.players.write().await;
        let passage = self.passage.read().await;
        if let (Some(player), Some(passage_text)) = (players.get_mut(player_id), passage.as_ref()) {
            let current_state = *self.state.read().await;
            if current_state != RracerState::Racing { return; }
            if player.is_bot { return; }
            if ts - player.last_keystroke < 20 { return; }
            player.last_keystroke = ts; player.keystroke_count += 1;
            if let Some(start) = player.start_time { let elapsed_seconds = (ts - start) as f64 / 1000.0; if elapsed_seconds > 0.1 { let current_wpm = gross_wpm(player.position, elapsed_seconds); if current_wpm > 300.0 { warn!("Suspicious typing speed from player {}: {} WPM", player_id, current_wpm); let _ = self.tx.send(ServerMsg::Error { message: "Suspicious typing speed detected".to_string() }); return; }}}
            if let Some(expected_char) = passage_text.chars().nth(player.position) {
                if ch == expected_char {
                    player.position += 1;
                    if player.start_time.is_none() { player.start_time = Some(ts); }
                    if player.position >= passage_text.len() {
                        player.finished = true;
                        let elapsed = (ts - player.start_time.unwrap_or(ts)) as f64 / 1000.0;
                        let wpm = net_wpm(player.position, elapsed, player.errors);
                        let acc = accuracy(player.position - player.errors, player.position);
                        let _ = self.tx.send(ServerMsg::Finish { id: player.name.clone(), wpm, accuracy: acc });
                    } else {
                        let _ = self.tx.send(ServerMsg::Progress { id: player.name.clone(), pos: player.position });
                    }
                } else { player.errors += 1; }
            }
        }
        let all_finished = players.values().all(|p| p.finished);
        if all_finished && !players.is_empty() {
            let mut state = self.state.write().await;
            if let Some(new_state) = RracerState::transition(&*state, &RracerEvent::AllDone) { *state = new_state; let _ = self.tx.send(ServerMsg::StateChange { state: "finished".to_string() }); }
        }
    }

    async fn tick(&self) {
        let current_state = *self.state.read().await;
        match current_state {
            RracerState::Waiting => { /* waiting handled in add/remove */ }
            RracerState::Countdown => {
                if let Some(start_time) = *self.countdown_start.read().await { let elapsed = current_timestamp() - start_time; if elapsed >= 3000 { let mut state = self.state.write().await; if let Some(new_state) = RracerState::transition(&*state, &RracerEvent::CountdownElapsed) { *state = new_state; if let Some(passage) = self.passage.read().await.as_ref() { let _ = self.tx.send(ServerMsg::Start { passage: passage.clone(), t0: current_timestamp(), }); } self.start_bots().await; info!("Room {} started racing", self.id); } } }
            }
            _ => {}
        }
    }

    async fn update_player_progress(&self, player_id: &str, position: usize) {
        let mut players = self.players.write().await;
        if let Some(player) = players.get_mut(player_id) {
            player.position = position;
            let _ = self.tx.send(ServerMsg::Progress { id: player.name.clone(), pos: position });
        }
    }

    async fn handle_player_finish(&self, player_id: &str, wpm: f64, accuracy: f64) {
        let mut players = self.players.write().await;
        if let Some(player) = players.get_mut(player_id) {
            player.finished = true;
            let _ = self.tx.send(ServerMsg::Finish { id: player.name.clone(), wpm, accuracy });
            let all_finished = players.values().all(|p| p.finished);
            if all_finished && !players.is_empty() {
                drop(players);
                let mut state = self.state.write().await;
                if let Some(new_state) = RracerState::transition(&*state, &RracerEvent::AllDone) { *state = new_state; let _ = self.tx.send(ServerMsg::StateChange { state: "finished".to_string() }); }
            }
        }
    }

    async fn start_bots(&self) {
        let passage_opt = self.passage.read().await.clone();
        let tx = self.tx.clone();
        let players_arc = self.players.clone();
        let state_arc = self.state.clone();
        if let Some(passage) = passage_opt {
            let len = passage.len();
            let snapshot: Vec<(String, String, f64)> = { let guard = players_arc.read().await; guard.iter().filter_map(|(id,p)| if p.is_bot { Some((id.clone(), p.name.clone(), p.bot_speed_wpm.unwrap_or(60.0))) } else { None }).collect() };
            for (bot_id, name, speed) in snapshot.into_iter() {
                let tx_clone = tx.clone(); let players_arc_clone = players_arc.clone(); let state_arc_clone = state_arc.clone();
                let cps = speed * 5.0 / 60.0;
                tokio::spawn(async move {
                    let mut pos: f64 = 0.0; let mut last = current_timestamp(); let tick = Duration::from_millis(100);
                    loop { tokio::time::sleep(tick).await; let now = current_timestamp(); let dt = (now - last) as f64 / 1000.0; last = now; pos += cps * dt; let mut ipos = pos.floor() as usize; if ipos > len { ipos = len; } let _ = tx_clone.send(ServerMsg::Progress { id: name.clone(), pos: ipos }); if ipos >= len { let wpm = speed; let acc = 100.0; let _ = tx_clone.send(ServerMsg::Finish { id: name.clone(), wpm, accuracy: acc }); { let mut guard = players_arc_clone.write().await; if let Some(p) = guard.get_mut(&bot_id) { p.finished = true; p.position = len; } let all_finished = guard.values().all(|p| p.finished); if all_finished && !guard.is_empty() { } } break; } }
                    let done = { let guard = players_arc_clone.read().await; guard.values().all(|p| p.finished) && !guard.is_empty() };
                    if done { if let Ok(mut state) = state_arc_clone.try_write() { if let Some(new_state) = RracerState::transition(&*state, &RracerEvent::AllDone) { *state = new_state; let _ = tx_clone.send(ServerMsg::StateChange { state: "finished".to_string() }); } } else { let _ = tx_clone.send(ServerMsg::StateChange { state: "finished".to_string() }); } }
                });
            }
        }
    }
}

fn current_timestamp() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64 }

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let db_url = std::env::var("DATABASE_URL").ok();
    let db_pool: Option<Arc<PgPool>> = if let Some(url) = db_url { match db::connect(&url).await { Ok(pool) => Some(Arc::new(pool)), Err(e) => { tracing::warn!("DB connection failed: {:?}", e); None } } } else { None };
    let rooms: Rooms = Arc::new(DashMap::new());
    let app_state = AppState { rooms: rooms.clone(), db: db_pool.clone() };
    let rooms_tick = rooms.clone();
    tokio::spawn(async move { let mut interval = interval(Duration::from_millis(50)); loop { interval.tick().await; for room in rooms_tick.iter() { room.value().tick().await; } } });
    let app = Router::new()
        .route("/ws", get(ws_handler))
        .nest_service("/", ServeDir::new("web/dist").fallback(ServeFile::new("web/dist/index.html")))
        .layer(CorsLayer::permissive())
        .with_state(app_state.clone());
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    info!("Server running on http://0.0.0.0:3000");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse { ws.on_upgrade(move |socket| handle_socket(socket, state)) }

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    let player_id = Uuid::new_v4().to_string();
    let mut current_room: Option<String> = None;
    let mut _player_name: Option<String> = None;
    let mut room_rx: Option<broadcast::Receiver<ServerMsg>> = None;
    info!("New WebSocket connection established for player {}", player_id);
    loop {
        tokio::select! {
            ws_msg = receiver.next() => {
                match ws_msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(client_msg) = serde_json::from_str::<ClientMsg>(&text) {
                            match client_msg {
                                ClientMsg::Join { room, name } => {
                                    if let Some(room_id) = &current_room { if let Some(room) = state.rooms.get(room_id) { room.remove_player(&player_id).await; } }
                                    let db_for_room = state.db.clone();
                                    let room = state.rooms.entry(room.clone()).or_insert_with(|| Room::new(room.clone(), db_for_room));
                                    room_rx = Some(room.tx.subscribe());
                                    let player = Player { id: player_id.clone(), name: name.clone(), position:0, start_time: None, last_keystroke:0, errors:0, finished:false, keystroke_count:0, is_bot:false, bot_speed_wpm: None };
                                    room.add_player(player).await;
                                    current_room = Some(room.id.clone());
                                    _player_name = Some(name);
                                    // Direct lobby snapshot for the joiner
                                    if let Ok(text) = { let g = room.players.read().await; let names: Vec<String> = g.values().map(|p| p.name.clone()).collect(); serde_json::to_string(&ServerMsg::Lobby { players: names }) } { let _ = sender.send(Message::Text(text)).await; }
                                }
                                ClientMsg::Key { ch, ts } => { if let Some(room_id) = &current_room { if let Some(room) = state.rooms.get(room_id) { room.handle_keystroke(&player_id, ch, ts).await; } } }
                                ClientMsg::Progress { pos, ts: _ } => { if let Some(room_id) = &current_room { if let Some(room) = state.rooms.get(room_id) { room.update_player_progress(&player_id, pos).await; } } }
                                ClientMsg::Finish { wpm, accuracy, time: _, ts: _ } => { if let Some(room_id) = &current_room { if let Some(room) = state.rooms.get(room_id) { room.handle_player_finish(&player_id, wpm, accuracy).await; } } }
                                ClientMsg::Reset => {
                                    if let Some(room_id) = &current_room { if let Some(room) = state.rooms.get(room_id) {
                                        if let Some(new_state) = { let state = room.state.read().await.clone(); RracerState::transition(&state, &RracerEvent::Reset) } { let mut state_w = room.state.write().await; *state_w = new_state; }
                                        *room.passage.write().await = None; *room.countdown_start.write().await = None; *room.waiting_start.write().await = None; room.last_timer_second.store(0, std::sync::atomic::Ordering::Relaxed);
                                        let mut players = room.players.write().await; players.retain(|_,p| !p.is_bot); for p in players.values_mut() { p.position=0; p.start_time=None; p.errors=0; p.finished=false; p.keystroke_count=0; } drop(players);
                                        let _ = room.tx.send(ServerMsg::StateChange { state: "waiting".to_string() }); room.broadcast_lobby().await; room.try_start_countdown().await;
                                    }}
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                    _ => {}
                }
            }
            room_msg = async { if let Some(ref mut rx) = room_rx { rx.recv().await } else { std::future::pending().await } } => {
                match room_msg { Ok(msg) => { if let Ok(text) = serde_json::to_string(&msg) { if sender.send(Message::Text(text)).await.is_err() { break; } } } Err(broadcast::error::RecvError::Closed) => break, Err(broadcast::error::RecvError::Lagged(_)) => continue }
            }
        }
    }
    if let Some(room_id) = &current_room { if let Some(room) = state.rooms.get(room_id) { room.remove_player(&player_id).await; } }
}
