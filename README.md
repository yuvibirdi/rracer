# RRACER 
Real‑time multiplayer typing racer.

## Stack


## Development
To build and run:\
`./setup.sh --run`\
Opens at http://localhost:3000

### Passages via Postgres (optional)

If you set `DATABASE_URL`, the server will select random passages from Postgres.

1. Start Postgres and set the env var:
	- macOS (Homebrew): `brew services start postgresql@16`
	- Create DB and user as desired, then export `DATABASE_URL`.
2. On server start, the schema is created automatically:
	- `passages(id serial, text text unique, source_url text, created_at timestamptz)`
3. Ingest passages from the web:
	- Put URLs in a file like `server/urls.txt` (one per line).
	- Run the ingestion tool:
	  - `cargo run -p server --bin ingest -- --file server/urls.txt`
	- Or pass URLs directly:
	  - `cargo run -p server --bin ingest -- https://example.com/article1 https://example.com/article2`

When Postgres is not configured or empty, the server falls back to the built-in static passages.
