# Multi-stage build for rracer
FROM rust:1.75 as builder

WORKDIR /app
COPY . .

# Build the server
RUN cargo build --release --bin server

# Final stage - minimal image
FROM alpine:latest

RUN apk --no-cache add ca-certificates

WORKDIR /app

# Copy the server binary
COPY --from=builder /app/target/release/server /app/server

# Copy the static web files
COPY --from=builder /app/web/static /app/web/static

EXPOSE 3000

CMD ["./server"]
