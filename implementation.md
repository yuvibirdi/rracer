Multiplayer start condition + bots plan (Aug 2025)

Goals

- Don’t start races instantly. Wait until there are at least 2 real humans in the same room.
- Auto-fill the lobby with bots so total participants >= 5, then begin the race.
- Keep the UX snappy: show passage at countdown via existing Countdown message; typing still gated until Start.

Scope of change

- Server-only logic for start conditions and bot simulation.
- No protocol changes required beyond fixing a minor inconsistency in Progress IDs (use player names, not UUIDs) so clients track lanes correctly.
- Frontend untouched functionally; it will naturally render bots via Lobby list and Progress events.

Design

1) Start conditions
	- Maintain a human_count (players with is_bot == false).
	- Transition Waiting -> Countdown only when human_count >= 2.
	- When transitioning, create enough bots to reach a total of min_total = 5 players (humans + bots).
	- Remove the 10s auto-start timer; races won’t start from a timer anymore.

2) Bot model
	- Represent bots as normal Player entries with is_bot = true and a target WPM (randomized per bot, e.g., 40–90 WPM).
	- On Start (Countdown -> Racing), spawn a short-lived async task per bot:
	  - Simulate progress at ~10 Hz using cps = wpm * 5 / 60.
	  - Increment position with fractional carry; clamp at passage length; send Progress updates.
	  - On finish, invoke the same Finish path as humans (so leaderboard and client UI work).
	- Bots are removed on Reset (so each race re-seeds fresh bots).

3) Protocol
	- Keep ServerMsg::{Lobby, Countdown, Start, Progress, Finish, StateChange, Error} as-is.
	- Fix: Progress.id will be the player’s display name (aligns with Lobby names + client rendering).

4) UX/Client
	- No UI changes. The client already:
	  - Shows passage on Countdown and enables typing on Start.
	  - Renders lanes from Lobby names and positions from Progress/Finish.
	- The waiting timer UI will simply not show a countdown (no auto-start timer anymore).

5) Edge cases
	- If a human disconnects during waiting leaving <2 humans, remain in Waiting.
	- If a late human joins during Countdown/Racing, they’ll see the current state (no bot changes mid-race).
	- Reset removes bots and returns to Waiting with only remaining humans.

Implementation checklist

- [ ] Add fields to Player: is_bot: bool, bot_speed_wpm: Option<f64>.
- [ ] Change server start condition: trigger Countdown only when human_count >= 2.
- [ ] Add bots to reach 5 total before Countdown; broadcast updated Lobby; send Countdown with passage.
- [ ] Remove 10s auto-start from Waiting tick; keep other states unchanged.
- [ ] Fix Progress messages to use player name as id.
- [ ] Spawn bot simulation tasks on Start; send Progress and Finish.
- [ ] On Reset, remove bots from the room; broadcast Lobby; clear race state.
- [ ] Add rand dependency for bot WPM.

Verification

- Two browsers connecting to the same room should trigger Countdown; lobby auto-fills with 3 bots to make 5 total.
- Lanes for both humans and bots should move; Progress IDs match names.
- Reset returns to Waiting, removes bots, and requires 2 humans again for next race.

—

Passages via Postgres + web ingestion (Aug 2025)

Goals

- Store passages in Postgres instead of static code.
- Ingest content from the web (URLs) into a passages table.
- Refactor runtime to fetch a random passage from DB; fallback to in-code list if DB is empty/unavailable.

Design

- Use sqlx (runtime-tokio, postgres, rustls) for async DB access.
- New table: passages(id SERIAL PRIMARY KEY, text TEXT NOT NULL, source_url TEXT, created_at TIMESTAMPTZ DEFAULT now()).
- On server startup, ensure table exists.
- Fetch random row: SELECT text FROM passages ORDER BY random() LIMIT 1.
- Fallback: use existing static PASSAGES if DB returns 0 rows or on error.
- Ingestion tool: standalone bin `server/bin/ingest.rs` to read URLs, fetch HTML (reqwest), extract <p> text (scraper), normalize, split into reasonable-length passages (e.g., 120–420 chars), and insert.

Why not put DB code in shared/src/passages.rs?

- The shared crate is compiled for WebAssembly (client) and cannot depend on native Postgres/reqwest.
- We keep DB access in server; shared/passages.rs remains as a fallback utility only. Server no longer calls it unless DB is empty.

Implementation checklist

- [ ] Add dependencies: sqlx (postgres, runtime-tokio-rustls), reqwest, scraper.
- [ ] server/src/db.rs: PgPool setup + ensure_table + fetch_random_passage(pool) -> Result<String>.
- [ ] server main: introduce AppState { rooms, pool }, pass Arc<PgPool> into Room::new.
- [ ] Replace get_random_passage() calls with DB-backed fetch + fallback to shared::passages.
- [ ] Add server/bin/ingest.rs reading `urls.txt` and inserting passages.
- [ ] Document DATABASE_URL usage and ingestion quickstart.

Verification

- With DATABASE_URL set and passages present, races use DB passages.
- With empty DB or no DB, races still work using fallback static passages.

