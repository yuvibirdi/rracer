# Multi-stage build for rracer
FROM rust:1.75 as builder

# Install wasm-pack and trunk for building the web client
RUN cargo install trunk wasm-bindgen-cli
RUN rustup target add wasm32-unknown-unknown

WORKDIR /app
COPY . .

# Build the web client
WORKDIR /app/web
RUN trunk build --release

# Build the server
WORKDIR /app
RUN cargo build --release --bin server

# Final stage - minimal image
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the server binary
COPY --from=builder /app/target/release/server /app/server

# Copy the web client dist
COPY --from=builder /app/web/dist /app/web/dist

EXPOSE 3000

CMD ["./server"]
