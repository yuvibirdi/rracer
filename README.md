# RRACER
- Real-time, multiplayer typing racer built in Rust with a minimal architecture.
- Can play with mutliple humans (multiple sever instances) and fills the remaining spots (up to 5) with bots.

## Minimal architecture
- `server/` — Rust backend binary (handles game rooms, websocket connections, and a Postgres passage store).
- `web/` — Rust → WASM frontend that connects to the server for real-time play.
- `shared/` — Shared Rust crate with message types, protocol definitions, and utilities used by both server and web.

## Development (quick start)
The repository includes a helper script to set up and run everything for development.

- Ingest passages into the postgres database:
```bash
# Put one URL per line in server/urls.txt
setup.sh --ingest-file server/urls.txt
```
- Build and run full app (server + web):

```bash
./setup.sh --run -r
```
for other options just run,

```bash
./setup.sh --help
```
When Postgres is not configured or the passages table is empty, the server falls back to bundled static passages.