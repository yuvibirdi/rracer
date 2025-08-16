# RRACER 
Real‑time multiplayer typing racer.

## Stack

- Backend: Rust, Axum, Tokio, tower-http
- Realtime: WebSockets (JSON protocol in `shared/`)
- Frontend: Rust + WASM (Leptos CSR), Tailwind (CDN)
- Build: Trunk (wasm32-unknown-unknown), Cargo workspaces
- Serve: SPA static from `web/dist` via server with SPA fallback

## Development
To build and run:\
`./setup.sh --run`\
Opens at http://localhost:3000
