# rracer 🏁

A real-time typing race game built with Rust and WebAssembly, inspired by TypeRacer.

## Features

- **Real-time multiplayer racing** - See other players' progress as they type
- **WebSocket-based communication** - Low-latency updates
- **Rust everywhere** - Shared game logic between client and server
- **WebAssembly frontend** - Fast, safe client-side execution
- **Responsive design** - Works on desktop and mobile

## Quick Start

### Prerequisites

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install WebAssembly target
rustup target add wasm32-unknown-unknown

# Install build tools
cargo install trunk wasm-bindgen-cli
```

### Development

1. **Start the server:**
   ```bash
   cd server
   cargo run --release
   ```

2. **Start the web client (in another terminal):**
   ```bash
   cd web
   trunk serve
   ```

3. **Open your browser:**
   - Go to `http://localhost:8080`
   - Open multiple tabs to test multiplayer functionality

### Production Build

```bash
# Build web client
cd web
trunk build --release

# Build server
cd ..
cargo build --release --bin server

# Run server (serves both API and static files)
./target/release/server
```

### Docker

```bash
# Build Docker image
docker build -t rracer .

# Run container
docker run -p 3000:3000 rracer
```

Then visit `http://localhost:3000`

## How to Play

1. **Enter a room name and your name**
2. **Click "Connect & Join"** to join a room
3. **Wait for other players** - races start automatically when 2+ players join
4. **Type the displayed passage** as quickly and accurately as possible
5. **Race to the finish!** - Your position updates in real-time

## Architecture

- **Frontend**: Leptos (Rust → WebAssembly) with reactive UI
- **Backend**: Axum + Tokio for async WebSocket handling  
- **Shared Logic**: Common Rust crates for game state, protocol, and WPM calculations
- **State Management**: Finite state machine with compile-time verified transitions

## Project Structure

```
rracer/
├── shared/           # Shared Rust library
│   ├── src/
│   │   ├── fsm.rs    # Game state machine
│   │   ├── protocol.rs # WebSocket message types
│   │   ├── wpm.rs    # WPM calculation utilities
│   │   └── passages.rs # Text passages for races
├── server/           # Backend server
│   └── src/main.rs   # Axum server with WebSocket support
├── web/              # Frontend (Leptos → WASM)
│   ├── src/
│   │   ├── app.rs    # Main UI component
│   │   └── websocket.rs # WebSocket client
│   ├── index.html    # HTML template
│   └── Trunk.toml    # Build configuration
└── Dockerfile        # Multi-stage container build
```

## Game States

The game follows a finite state machine:

```
Waiting → Countdown → Racing → Finished
   ↑                              ↓
   ←──────────── Reset ←──────────
```

- **Waiting**: Players can join, waiting for minimum 2 players
- **Countdown**: 3-second countdown before race starts  
- **Racing**: Players type the passage, positions update in real-time
- **Finished**: Race complete, show results and option to reset

## Performance

- **Sub-50ms latency** for keystroke updates via WebSockets
- **60 FPS rendering** with fine-grained reactivity (only changed elements re-render)
- **Memory efficient** - shared Rust structs, minimal JavaScript overhead
- **Scalable** - async architecture handles thousands of concurrent connections

## Security Features

- **Server-authoritative** - All keystrokes validated server-side
- **Rate limiting** - Prevents >20 keystrokes per 100ms
- **Input validation** - Malformed messages are rejected
- **CORS protection** - Configurable cross-origin policies

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Test with `cargo test` and manual testing
5. Submit a pull request

## License

MIT License - see LICENSE file for details.

---

Built with ❤️ in Rust 🦀
