# rracer

A real-time multiplayer typing race game built with Rust and WebSockets.

## Stack

- **Backend**: Rust with Tokio for async WebSocket handling
- **Frontend**: Vanilla HTML/CSS/JavaScript
- **WebSockets**: Real-time communication between players
- **Build**: Cargo for Rust compilation

## Build and Deploy

### Development

1. Clone the repository
2. Build the server:
   ```bash
   cargo build
   ```
3. Run the server:
   ```bash
   cargo run --bin server
   ```
4. Open http://localhost:3000 in your browser

### Production

1. Build the release version:
   ```bash
   cargo build --release
   ```
2. Run the server:
   ```bash
   ./target/release/server
   ```

The server serves static files from the `web/static/` directory and handles WebSocket connections on the same port.