# rracer

A real-time multiplayer typing race game built with Rust and WebSockets.
<!-- I initially wanted to make this thing a rust wasm which I'll do at a later date if I feel like it -->
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
   ./setup.sh --run
   ```
3. Run the server:
   ```bash
   cargo run -p server
   ```
4. Open http://localhost:3000 in your browser
