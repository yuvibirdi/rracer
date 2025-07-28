use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ClientMsg {
    Join { room: String, name: String },
    Key { ch: char, ts: u64 },
    Reset,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ServerMsg {
    Lobby { players: Vec<String> },
    Start { passage: String, t0: u64 },
    Progress { id: String, pos: usize },
    Finish { id: String, wpm: f64, accuracy: f64 },
    Error { message: String },
    StateChange { state: String },
}
