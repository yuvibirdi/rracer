# rracer – A Typeracer Clone in Rust & WebAssembly

## 1 Overview
rracer is a browser-based real-time typing race that faithfully reproduces the core game-play of [TypeRacer](https://play.typeracer.com)[19].  All game logic is written in safe Rust, compiled to WebAssembly (Wasm) for the front-end, and re-used on the back-end for authoritative state management.  WebSockets provide low-latency bi-directional updates so that racers see each other’s cars advance as soon as keystrokes are validated.

The design emphasises:
* **One language, one codebase** – both client and server share Rust crates.
* **Fine-grained reactivity** – Yew/Leptos render only the DOM nodes that change.
* **Predictable game state** – a formal finite-state machine guarantees legal transitions.
* **Scalable networking** – Axum + Tokio-Tungstenite handle tens of thousands of concurrent sockets.
* **Zero-install play** – shipping as static Wasm + JS runs on any modern browser.

---

## 2 Technology Stack
| Layer | Choice | Why |
|-------|--------|-----|
| Front-end | `leptos` 0.7 / `yew` 0.21[65][73] | Fast CSR/SSR, JSX-like `html!` macro, SSR hydration for SEO |
| WebAssembly tool | `wasm-pack` & `trunk`[21][25] | One-command build → `pkg/` JS glue & `.wasm` |
| Real-time transport | WebSocket (RFC 6455) | Persistent, low-latency, fits game tick model |
| Server framework | `axum` 0.7 + `tokio-tungstenite` 0.27[100][114] | Ergonomic routing + async sockets |
| State machine | `rust-fsm` 0.8[102][112] | Compile-time checked transitions |
| Persistence | SQLite via `sqlx` | Single-file DB good for small leaderboards |
| Packaging | GitHub Actions → Docker scratch image | Multi-platform, 10 MB image |

---

## 3 Workspace Layout
```
rracer/
├─ Cargo.toml            # workspace
├─ shared/               # crate reused by client & server
│   ├─ passages.rs        # static & user-submitted texts
│   ├─ wpm.rs             # speed & accuracy utilities
│   └─ fsm.rs             # generated state machine
├─ web/                  # client (leptos/yew) → Wasm
│   ├─ src/
│   │   └─ app.rs         # <App /> component tree
│   └─ Trunk.toml
└─ server/
    └─ src/main.rs        # axum + WebSocket hub
```
Make a new workspace:
```bash
cargo new --lib shared
cargo new --lib web
cargo new server
```
Add to **root `Cargo.toml`**:
```toml
[workspace]
members = ["shared", "web", "server"]
```

---

## 4 Shared Crate Details
### 4.1 State Machine
```rust
fsm! {
    rracer(State);
    *Waiting --> Countdown    on Join;
    Countdown --> Racing      on CountdownElapsed;
    Racing    --> Finished    on AllDone;
    Finished  --> Waiting     on Reset;
}
```
`rust-fsm` generates an enum `rracerState` and a `transition()` method with exhaustive `match`, so illegal hops fail to compile.

### 4.2 WPM & Accuracy
```rust
/// Gross words per minute (no error penalty)
pub fn gross_wpm(chars: usize, seconds: f64) -> f64 {
    (chars as f64 / 5.0) / (seconds / 60.0)
}
/// Net WPM = gross – unfixed errors penalty[116]
pub fn net_wpm(chars: usize, seconds: f64, errors: usize) -> f64 {
    gross_wpm(chars, seconds) - errors as f64 * 60.0 / seconds
}
```
Formula derives from industry standard where one *word* = 5 keystrokes[82][91].

### 4.3 Protocol Types
```rust
#[derive(Serialize, Deserialize)]
pub enum ClientMsg {
    Join { room: String, name: String },
    Key { ch: char, ts: u64 },
}
#[derive(Serialize, Deserialize)]
pub enum ServerMsg {
    Lobby { players: Vec<String> },
    Start { passage: String, t0: u64 },
    Progress { id: String, pos: usize },
    Finish { id: String, wpm: f64 },
}
```

---

## 5 Server (Axum)
```rust
#[tokio::main]
async fn main() {
    let rooms = Arc::<DashMap<String, Room>>::default();
    let app = Router::new()
        .route("/ws", get(ws_handler))
        .with_state(rooms);
    axum::Server::bind(&"0.0.0.0:3000".parse()?)
        .serve(app.into_make_service())
        .await?;
}
```
### 5.1 WebSocket Handler
```rust
async fn ws_handler(
    ws: WebSocketUpgrade,
    State(rooms): State<Rooms>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| client(socket, rooms))
}
```
`client()` splits the socket (read/write)[105] and registers the user with a `broadcast::Sender`. Each lobby is its own channel so messages wake only relevant tasks[115].

### 5.2 Tick Loop
Every 50 ms the room loops over players, computes `Progress` diff, and broadcasts.  Game end is triggered when `pos == passage.len()` for all racers ➜ `AllDone` event.

---

## 6 Front-end (Leptos)
### 6.1 Bootstrapping
```rust
#[component]
fn App() -> impl IntoView {
    let ws = create_websocket("ws://".to_string() + &window().location().host().unwrap() + "/ws");
    provide_context(ws.clone());
    view! {
        <Router>
            <Lobby/>
            <Race/>
            <Results/>
        </Router>
    }
}
```
### 6.2 Reactive Store
```rust
#[derive(Clone, Default)]
struct RaceState { passage: RwSignal<String>, cursors: RwSignal<HashMap<String, usize>>, ... }
```
Signals trigger re-render of only the car div that moved, achieving 60 fps on low-end mobiles.

### 6.3 Keystroke Validation
```rust
on:keydown=move |ev| {
    let ch = ev.key().chars().next()?;
    if passage.get().as_bytes()[index.get()] as char == ch {
        index.update(|i| *i += 1);
        ws.send(ClientMsg::Key{ch,ts: now()});
    } else {
        errors.update(|e| *e += 1);
    }
}
```

---

## 7 Building & Running
```bash
# toolchain
rustup target add wasm32-unknown-unknown
cargo install trunk wasm-bindgen-cli wasm-pack

# dev server (hot reload)
cd web
trunk serve

# back-end
cd ../server
cargo run --release
```
Navigate to `http://localhost:8080` ⇒ open two tabs ⇒ race!

For production build:
```bash
trunk build --release
docker build -t rracer .  # multi-stage copying ./dist + server binary
```

---

## 8 Game State Timeline
```
┌────────┐   Join   ┌────────────┐  CountdownElapsed  ┌────────┐   AllDone   ┌─────────┐
│Waiting │ ───────▶ │ Countdown  │ ─────────────────▶ │ Racing │ ───────────▶ │Finished │
└────────┘          └────────────┘                    └────────┘              └─────────┘
```
Illegal transitions are impossible at compile time thanks to `rust-fsm`.

---

## 9 Security & Anti-Cheat
* **Server authoritative**: server validates each `Key` matches passage char, discards invalid.
* **CAPTCHA on join** to limit bot abuse.
* **Rate limit**: drop client if >20 keystrokes in 100 ms.
* **Checksum**: hash of passage + order to prevent client predicting text early.

---

## 10 Extending
* **Custom passages**: POST `/api/passage` → pending moderation.
* **Mobile UI**: leverage Leptos SSR + Tailwind responsive utilities.
* **OAuth login**: add Axum OIDC middleware; store PBKDF2 hashes.
* **Analytics**: emit `finish` events to ClickHouse for percentile charts.

---

## 11 References
[19] TypeRacer – Wikipedia  
[21] MDN Guide: *Compiling from Rust to WebAssembly*  
[25] wasm-pack docs  
[65] Flosse – *Rust Web Framework Comparison*  
[73] `yew` crate docs  
[82] The Tech Advocate – *3 Ways to Calculate WPM*  
[91] Typing.com – *What is Words Per Minute*  
[100] Momori Nakano – *Building a WebSocket Chat with Axum*  
[102] `rust-fsm` crate  
[105] Axum docs – `extract::ws`  
[114] `tokio-tungstenite` crate  
[115] users.rust-lang.org – *Axum chat rooms discussion*  
[116] SpeedTypingOnline – *Typing Equations*
