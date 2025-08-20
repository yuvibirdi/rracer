use leptos::prelude::*;
use shared::protocol::{ClientMsg, ServerMsg};
use std::collections::HashMap;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{HtmlElement, WebSocket};
use std::cell::RefCell;
use crate::normalize::{normalize_char, is_skippable};
// no std::rc needed

// Thread-local storage for the active WebSocket. This avoids capturing non-Send/Sync
// types inside Leptos children closures, which require Fn + Send + Sync.
thread_local! { static WS_REF: RefCell<Option<WebSocket>> = const { RefCell::new(None) }; }
// Only enable testing UI in debug builds
const ALLOW_TEST_UI: bool = cfg!(debug_assertions);

#[component]
pub fn App() -> impl IntoView {
    let (game_state, set_game_state) = signal("waiting".to_string());
    let (players, set_players) = signal(Vec::<String>::new());
    let (passage, set_passage) = signal(String::new());
    let (player_positions, set_player_positions) = signal(HashMap::<String, usize>::new());
    let (current_position, set_current_position) = signal(0usize);
    let (errors, set_errors) = signal(0usize);
    let (start_time, set_start_time) = signal(None::<f64>);
    let (last_progress_sent, set_last_progress_sent) = signal(0.0f64);
    let (room_name, set_room_name) = signal("main".to_string());
    let (player_name, set_player_name) = signal("Player".to_string());
    let (connected, set_connected) = signal(false);
    let (_error_message, set_error_message) = signal(None::<String>);
    let (wpm, set_wpm) = signal(0.0);
    let (accuracy, set_accuracy) = signal(100.0);
    let (time_elapsed, set_time_elapsed) = signal(0.0f64);
    let (waiting_seconds, set_waiting_seconds) = signal(0u64);
    let (joined, set_joined) = signal(false);
    let (connecting, set_connecting) = signal(false);
    let (finish_time, set_finish_time) = signal(None::<f64>);
    let (leaderboard, set_leaderboard) = signal(Vec::<(String, f64, f64)>::new());
    let (test_mode, set_test_mode) = signal(false);
    let (debug_flag, set_debug_flag) = signal(false);
    
    // WebSocket is managed via thread-local storage (WS_REF)

    // Lightweight timer loop: update elapsed time every 100ms using server t0
    {
        let game_state_sig = game_state;
        let start_time_sig = start_time;
        let set_time_elapsed_sig = set_time_elapsed;
        if let Some(win) = web_sys::window() {
            let cb = Closure::wrap(Box::new(move || {
                if game_state_sig.get_untracked() == "racing" {
                    if let Some(t0_ms) = start_time_sig.get_untracked() {
                        let now_ms = js_sys::Date::now();
                        let elapsed = (now_ms - t0_ms) / 1000.0;
                        if elapsed >= 0.0 {
                            set_time_elapsed_sig.set(elapsed);
                        }
                    }
                }
            }) as Box<dyn FnMut()>);
            let _ = win.set_interval_with_callback_and_timeout_and_arguments_0(cb.as_ref().unchecked_ref(), 100);
            cb.forget();
        }
    }

    let connect_websocket = {
        move || {
            let win = web_sys::window().unwrap();
            let loc = win.location();
            let host = loc.host().unwrap();
            let protocol = loc.protocol().unwrap_or_else(|_| "http:".into());
            let ws_scheme = if protocol == "https:" { "wss" } else { "ws" };
            let ws_url = format!("{ws_scheme}://{host}/ws");
            
            match WebSocket::new(&ws_url) {
                Ok(ws) => {
                    set_connecting.set(true);
                    // Join on open; mark as connected then
                    {
                        let room_name_sig = room_name;
                        let player_name_sig = player_name;
                        let set_connected_cb = set_connected;
                        let set_joined_cb = set_joined;
                        let set_connecting_cb = set_connecting;
                        let onopen = Closure::wrap(Box::new(move || {
                            set_connected_cb.set(true);
                            set_connecting_cb.set(false);
                            // Auto-join the room once the socket is open
                            let msg = ClientMsg::Join { room: room_name_sig.get(), name: player_name_sig.get() };
                            if let Ok(json) = serde_json::to_string(&msg) {
                                // Best-effort send
                                WS_REF.with(|cell| {
                                    if let Some(ws) = cell.borrow().as_ref() { let _ = ws.send_with_str(&json); }
                                });
                            }
                            set_joined_cb.set(true);
                        }) as Box<dyn FnMut()>);
                        ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
                        onopen.forget();
                    }

                    // Handle close -> mark disconnected
                    {
                        let set_connected_cb = set_connected;
                        let set_state_cb = set_game_state;
                        let set_joined_cb = set_joined;
                        let set_connecting_cb = set_connecting;
                        let onclose = Closure::wrap(Box::new(move |_e: web_sys::CloseEvent| {
                            set_connected_cb.set(false);
                            set_state_cb.set("waiting".to_string());
                            set_joined_cb.set(false);
                            set_connecting_cb.set(false);
                        }) as Box<dyn FnMut(_)>);
                        ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
                        onclose.forget();
                    }
                    // Set up message handler
                    let onmessage_callback = {
                        let set_players = set_players;
                        let set_passage = set_passage;
                        let set_game_state = set_game_state;
                        let set_start_time = set_start_time;
                        let set_current_position = set_current_position;
                        let set_errors = set_errors;
                        let set_player_positions = set_player_positions;
                        let set_wpm = set_wpm;
                        let set_accuracy = set_accuracy;
                        let set_time_elapsed_cb = set_time_elapsed;
                        let set_error_message = set_error_message;
                        let set_player_positions2 = set_player_positions;
                        let player_name_signal = player_name;
                        let set_leaderboard_cb = set_leaderboard;
                        let set_finish_time_cb = set_finish_time;
                        let my_name_for_finish = player_name;
                        let test_mode_sig = test_mode;
                        
                        Closure::wrap(Box::new(move |e: web_sys::MessageEvent| {
                            if let Some(text) = e.data().as_string() {
                                if let Ok(msg) = serde_json::from_str::<ServerMsg>(&text) {
                                    if test_mode_sig.get_untracked() {
                                        // Ignore server-driven flow while in local test mode, except errors
                                        if !matches!(msg, ServerMsg::Error { .. }) { return; }
                                    }
                                    match msg {
                                        ServerMsg::Lobby { players: p } => {
                                            web_sys::console::log_1(&format!("Lobby update: {} players", p.len()).into());
                                            set_players.set(p);
                                        }
                                        ServerMsg::Countdown { passage: p } => {
                                            // Prepare passage early so UI can render instantly
                                            set_passage.set(p);
                                            set_game_state.set("countdown".to_string());
                                            set_current_position.set(0);
                                            set_errors.set(0);
                                            set_wpm.set(0.0);
                                            set_accuracy.set(100.0);
                                            set_last_progress_sent.set(0.0);
                                            set_player_positions2.set(HashMap::new());
                                            let me = player_name_signal.get();
                                            set_player_positions2.update(|m| { m.insert(me, 0); });
                                        }
                                        ServerMsg::Start { passage: p, t0 } => {
                                            set_passage.set(p);
                                            set_game_state.set("racing".to_string());
                                            // Use server start time for sync across clients
                                            set_start_time.set(Some(t0 as f64));
                                            set_time_elapsed_cb.set(0.0);
                                            set_current_position.set(0);
                                            set_errors.set(0);
                                            set_wpm.set(0.0);
                                            set_accuracy.set(100.0);
                                            set_last_progress_sent.set(0.0);
                                            set_player_positions2.set(HashMap::new());
                                            // Initialize our own lane position to 0 for immediate render
                                            let me = player_name_signal.get();
                                            set_player_positions2.update(|m| { m.insert(me, 0); });
                                            set_waiting_seconds.set(0);
                                            set_finish_time_cb.set(None);
                                            set_leaderboard_cb.set(Vec::new());

                                            // Focus the typing area if present
                                            if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
                                                if let Some(elem) = doc.get_element_by_id("typingArea") {
                                                    if let Ok(html) = elem.dyn_into::<HtmlElement>() {
                                                        let _ = html.focus();
                                                    }
                                                }
                                            }
                                        }
                                        ServerMsg::Progress { id, pos } => {
                                            set_player_positions.update(|positions| {
                                                positions.insert(id, pos);
                                            });
                                        }
                                        ServerMsg::Finish { id, wpm: player_wpm, accuracy: player_accuracy } => {
                                            web_sys::console::log_1(&format!("Player {id} finished with {player_wpm} WPM, {player_accuracy}% accuracy").into());
                                            // Update leaderboard, append in arrival order
                                            set_leaderboard_cb.update(|lb| lb.push((id.clone(), player_wpm, player_accuracy)));
                                            // If this is me, update my stats and move to finished state
                                            if id == my_name_for_finish.get() {
                                                set_wpm.set(player_wpm);
                                                set_accuracy.set(player_accuracy);
                                                set_game_state.set("finished".to_string());
                                            }
                                        }
                    ServerMsg::StateChange { state } => {
                                            let is_waiting = state == "waiting";
                                            set_game_state.set(state);
                                            if is_waiting {
                                                set_current_position.set(0);
                                                set_errors.set(0);
                                                set_wpm.set(0.0);
                                                set_accuracy.set(100.0);
                        set_error_message.set(None);
                        set_waiting_seconds.set(0);
                                                set_finish_time_cb.set(None);
                                                set_leaderboard_cb.set(Vec::new());
                                            }
                                        }
                                         ServerMsg::WaitingTimer { seconds_left } => {
                                             set_waiting_seconds.set(seconds_left);
                                             if seconds_left == 0 && game_state.get() == "waiting" {
                                                 // Move to a lightweight countdown state so the race UI shows instantly
                                                 set_game_state.set("countdown".to_string());
                                             }
                                         }
                                        ServerMsg::Error { message } => {
                                            set_error_message.set(Some(message.clone()));
                                            web_sys::console::error_1(&message.into());
                                        }
                                    }
                                } else {
                                    web_sys::console::error_1(&"Failed to parse ServerMsg JSON".into());
                                }
                            }
                        }) as Box<dyn FnMut(_)>)
                    };
                    
                    ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
                    onmessage_callback.forget();
                    
                    WS_REF.with(|cell| {
                        *cell.borrow_mut() = Some(ws);
                    });
                }
                Err(_) => {
                    web_sys::console::error_1(&"Failed to connect to WebSocket".into());
                }
            }
        }
    };

    let join_room = {
        move || {
        WS_REF.with(|cell| {
                if let Some(ws) = cell.borrow().as_ref() {
                    let msg = ClientMsg::Join {
                        room: room_name.get(),
                        name: player_name.get(),
                    };
                    if let Ok(json) = serde_json::to_string(&msg) {
                        let _ = ws.send_with_str(&json);
                    }
            set_joined.set(true);
                }
            });
        }
    };

    view! {
        <div class="bg min-h-screen">
            <div class="container mx-auto p-4 max-w-6xl">
                <div class="text-center mb-8">
                    <h1 class="text-5xl font-bold text-white mb-2">"🏁 rracer"</h1>
                    <p class="text-white text-lg">"Real-time multiplayer typing races"</p>
                </div>

                <div class="stat-card rounded-xl shadow-xl p-6 mb-6">
                    <div class="flex gap-4 mb-4">
                        <input type="text" placeholder="Room name" class="border-2 border-gray-200 rounded-lg px-4 py-3 flex-1 focus:border-blue-500 focus:outline-none transition-colors" prop:value=room_name on:input=move |ev| set_room_name.set(event_target_value(&ev))/>
                        <input type="text" placeholder="Your name" class="border-2 border-gray-200 rounded-lg px-4 py-3 flex-1 focus:border-blue-500 focus:outline-none transition-colors" prop:value=player_name on:input=move |ev| set_player_name.set(event_target_value(&ev))/>
                        <button class="bg text-white px-6 py-3 rounded-lg hover:bg-blue-600 transition-colors font-semibold disabled:opacity-50 disabled:cursor-not-allowed"
                            on:click=move |_| {
                                if joined.get() || connecting.get() { return; }
                                if !connected.get() { connect_websocket(); } else { join_room(); }
                            }
                            prop:disabled=move || joined.get() || connecting.get()>
                            {move || if joined.get() { "Joined" } else if connected.get() { "Join Room" } else { "Connect & Join" }}
                        </button>
                        <Show when=|| ALLOW_TEST_UI>
                            <button class="bg-gray-700 text-white px-6 py-3 rounded-lg hover:bg-gray-800 transition-colors font-semibold"
                                on:click=move |_| {
                                    set_test_mode.set(true);
                                    set_passage.set(crate::normalize::tests_passage());
                                    set_game_state.set("racing".to_string());
                                    set_start_time.set(Some(js_sys::Date::now()));
                                    set_current_position.set(0);
                                    set_errors.set(0);
                                    set_wpm.set(0.0);
                                    set_accuracy.set(100.0);
                                    set_last_progress_sent.set(0.0);
                                    set_player_positions.set(HashMap::new());
                                    let me = player_name.get();
                                    set_players.set(vec![me.clone()]);
                                    set_player_positions.update(|m| { m.insert(me, 0); });
                                    set_waiting_seconds.set(0);
                                    set_finish_time.set(None);
                                    set_leaderboard.set(Vec::new());
                                }>
                                {move || if test_mode.get() { "Test Text Loaded" } else { "Load Test Text" }}
                            </button>
                            <button class="bg-gray-600 text-white px-4 py-3 rounded-lg hover:bg-gray-700 transition-colors font-semibold"
                                on:click=move |_| { set_debug_flag.update(|d| *d = !*d); }>
                                {move || if debug_flag.get() { "Debug: ON" } else { "Debug: OFF" }}
                            </button>
                        </Show>
                    </div>
                    <div class="text-sm text-gray-600">
                        "Status: "<span class="font-semibold">{move || if connected.get() { "Connected".to_string() } else { "Disconnected".to_string() }}</span>
                    </div>
                </div>

        <Show when=move || _error_message.get().is_some()>
                    <div class="bg-red-100 border-2 border-red-400 text-red-700 p-4 rounded-lg mb-6">
            {move || _error_message.get().unwrap_or_default()}
                    </div>
                </Show>


                <Show when=move || {
                    let s = game_state.get();
                    s == "racing" || s == "countdown"
                }>
                    <div class="stat-card rounded-xl shadow-xl p-6 mb-6">
                        <div class="flex justify-between items-center mb-4">
                            <h2 class="text-2xl font-bold text-gray-800">"🏁 Race in Progress"</h2>
                            <div class="flex gap-6">
                                <div class="text-center">
                                    <div class="text-3xl font-bold text-blue-600">{move || format!("{:.0}", wpm.get())}</div>
                                    <div class="text-sm text-gray-500">"WPM"</div>
                                </div>
                                <div class="text-center">
                                    <div class="text-3xl font-bold text-green-600">{move || format!("{:.0}%", accuracy.get())}</div>
                                    <div class="text-sm text-gray-500">"Accuracy"</div>
                                </div>
                                <div class="text-center">
                                    <div class="text-3xl font-bold text-purple-600">{move || format!("{:.1}s", time_elapsed.get())}</div>
                                    <div class="text-sm text-gray-500">"Time"</div>
                                </div>
                            </div>
                        </div>
                        <div class="race-track mb-6" style="min-height: 240px;">
                            <div class="finish-line"></div>
                            <For
                                each=move || players.get().into_iter().enumerate()
                                key=|(i, p)| format!("{i}-{p}")
                                children=move |(idx, player)| {
                                    let player_for_pos = player.clone();
                                    let player_for_self = player.clone();
                                    let position = move || player_positions.get().get(&player_for_pos).copied().unwrap_or(0);
                                    let total = move || passage.get().len().max(1);
                                    let percent = move || (position() as f64 / total() as f64) * 95.0;
                                    let is_self = move || player_for_self == player_name.get();
                                    let car_class = move || {
                                        if is_self() { "car car-player".to_string() } else {
                                            match idx % 4 {
                                                0 => "car car-opponent1".to_string(),
                                                1 => "car car-opponent2".to_string(),
                                                2 => "car car-opponent3".to_string(),
                                                _ => "car car-opponent4".to_string(),
                                            }
                                        }
                                    };
                                    let label = player.clone();
                                    view! {
                                        <div class="race-lane">
                                            <div class=car_class style=move || format!("left: {}%;", percent())>
                                                "🚗"
                                            </div>
                                            <div class="ml-14 pl-10 text-gray-700 font-medium">{label}</div>
                                        </div>
                                    }
                                }
                            />
                        </div>
                        <div class="mb-4">
                            <h3 class="text-lg font-semibold mb-2 text-gray-700">"Type this passage:"</h3>
                            <p class="text-xs text-gray-500 mb-2">"Tip: type straight quotes (\" '), hyphen (-), and space for curly quotes, long dashes, and non‑breaking spaces."</p>
                <div id="typingArea" class="text-xl font-mono leading-relaxed p-6 bg-white rounded-lg border-2 border-gray-200 typing-area min-h-[120px] passage-text" tabindex="0"
                                on:keydown=move |ev: web_sys::KeyboardEvent| {
                    // Only handle typing once the race has actually started
                    if game_state.get() != "racing" { return; }
                    if start_time.get().is_none() { return; }
                                    // Ignore modifier combos and non-character keys
                                    if ev.ctrl_key() || ev.meta_key() || ev.alt_key() { return; }
                                    let key = ev.key();
                                    // Only process single-character keys
                                    if key.chars().count() != 1 {
                                        if debug_flag.get() || test_mode.get() {
                                            web_sys::console::log_1(&format!("IGNORED (non-char): key='{}' code='{}'", key, ev.code()).into());
                                        }
                                        return;
                                    }
                                    ev.prevent_default();
                                    if let Some(ch_raw) = key.chars().next() {
                                        // Normalize typed key (covers cases where browser reports a fancy char)
                                        let ch = normalize_char(ch_raw);
                                        let passage_text = passage.get();
                                        let cur_pos = current_position.get();
                                        if let Some(expected_char) = passage_text.chars().nth(cur_pos) {
                                            // If the expected passage char is a skippable invisible, advance automatically
                                            if is_skippable(expected_char) {
                                                if debug_flag.get() || test_mode.get() {
                                                    web_sys::console::log_1(&format!(
                                                        "SKIP invisible at pos {}: expected='{}' (U+{:04X})",
                                                        cur_pos,
                                                        expected_char,
                                                        expected_char as u32
                                                    ).into());
                                                }
                                                set_current_position.set(cur_pos + 1);
                                                return;
                                            }
                                            let typed_norm = ch;
                                            let expected_norm = normalize_char(expected_char);
                                            if debug_flag.get() || test_mode.get() {
                                                web_sys::console::log_1(&format!(
                                                    "COMPARE pos {} => raw='{}' (U+{:04X}) -> typed_norm='{}' (U+{:04X}); expected='{}' (U+{:04X}) -> expected_norm='{}' (U+{:04X}); equal={}",
                                                    cur_pos,
                                                    ch_raw,
                                                    ch_raw as u32,
                                                    typed_norm,
                                                    typed_norm as u32,
                                                    expected_char,
                                                    expected_char as u32,
                                                    expected_norm,
                                                    expected_norm as u32,
                                                    typed_norm == expected_norm
                                                ).into());
                                            }
                        if typed_norm == expected_norm {
                                                let next_pos = cur_pos + 1;
                                                set_current_position.set(next_pos);

                                                // Update local car position immediately
                                                let me = player_name.get();
                                                set_player_positions.update(|m| { m.insert(me.clone(), next_pos); });

                        // Update realtime WPM & accuracy
                                                if let Some(start) = start_time.get() {
                                                    let now = js_sys::Date::now();
                                                    // seconds (server-synced), clamp to avoid zero/negative due to clock skew
                                                    let elapsed = ((now - start) / 1000.0).max(0.1);
                                                    if elapsed > 0.0 {
                                                        // Monkeytype-style WPM: only correct chars, no error penalty subtraction
                                                        let chars_typed = next_pos;
                                                        let wpm_now = (chars_typed as f64 / 5.0) / (elapsed / 60.0);
                            set_wpm.set(wpm_now.max(0.0));

                                                        let total_chars = chars_typed + errors.get();
                                                        if total_chars > 0 { set_accuracy.set((chars_typed as f64 / total_chars as f64) * 100.0); }
                                                    }
                                                    // Throttle progress messages (>=100ms between sends)
                                                    let last = last_progress_sent.get();
                                                    if now - last >= 100.0 {
                                                        if !test_mode.get() {
                                                            WS_REF.with(|cell| {
                                                                if let Some(ws) = cell.borrow().as_ref() {
                                                                    let msg = ClientMsg::Progress { pos: next_pos, ts: now as u64 };
                                                                    if let Ok(json) = serde_json::to_string(&msg) { let _ = ws.send_with_str(&json); }
                                                                }
                                                            });
                                                        }
                                                        set_last_progress_sent.set(now);
                                                    }
                                                }

                                                // If finished, send Finish
                        if next_pos >= passage_text.chars().count() {
                                                    if let Some(start) = start_time.get() {
                                                        let now = js_sys::Date::now();
                                                        // seconds (server-synced), clamp
                                                        let elapsed = ((now - start) / 1000.0).max(0.1);
                            // Recompute WPM/accuracy at finish to avoid stale 0s
                                                        let chars_typed = next_pos;
                            let w = if elapsed > 0.0 { (chars_typed as f64 / 5.0) / (elapsed / 60.0) } else { 0.0 };
                            let a = if (chars_typed + errors.get()) > 0 { (chars_typed as f64 / (chars_typed + errors.get()) as f64) * 100.0 } else { 100.0 };
                            set_wpm.set(w.max(0.0));
                            set_accuracy.set(a);
                            set_finish_time.set(Some(elapsed));
                                                        if !test_mode.get() {
                                                            WS_REF.with(|cell| {
                                                                if let Some(ws) = cell.borrow().as_ref() {
                                                                    let msg = ClientMsg::Finish { wpm: w, accuracy: a, time: elapsed, ts: now as u64 };
                                                                    if let Ok(json) = serde_json::to_string(&msg) { let _ = ws.send_with_str(&json); }
                                                                }
                                                            });
                                                        }
                                                    }
                                                }
                                            } else {
                                                set_errors.update(|e| *e += 1);
                                                // Update accuracy on error
                                                let total_chars = current_position.get() + errors.get();
                                                if total_chars > 0 { set_accuracy.set((current_position.get() as f64 / total_chars as f64) * 100.0); }
                                            }
                                        }
                                    }
                                }>
                                <span class="correct-char">{move || passage.get().chars().take(current_position.get()).collect::<String>()}</span>
                                <span class="current-char">{move || passage.get().chars().nth(current_position.get()).unwrap_or(' ')}</span>
                                <span>{move || passage.get().chars().skip(current_position.get() + 1).collect::<String>()}</span>
                            </div>
                        </div>
                        <div class="flex justify-between text-sm text-gray-600 bg-gray-50 rounded-lg p-3">
                            <span>"Progress: "<span class="font-semibold">{current_position}</span>" / "<span class="font-semibold">{move || passage.get().len()}</span>" characters"</span>
                            <span>"Errors: "<span class="font-semibold text-red-600">{errors}</span></span>
                            <span>"Rank: "<span class="font-semibold text-blue-600">"#1"</span></span>
                        </div>
                    </div>
                </Show>

                <Show when=move || game_state.get() == "waiting">
                    <div class="stat-card rounded-xl shadow-xl p-6 mb-6">
                        <div class="text-center">
                            <h2 class="text-2xl font-bold text-gray-800 mb-4">"🏁 Waiting for Race"</h2>
                            <div class="text-gray-600 mb-6">
                                <p class="text-lg">"Waiting for more players to join..."</p>
                                <p class="text-sm mt-2">"Race starts when 2+ players join the room"</p>
                                <Show when=move || (waiting_seconds.get() > 0)>
                                    <div class="mt-4 p-3 bg-gray-50 rounded-lg inline-block">
                                        <p class="text-gray-800 font-semibold">{move || format!("Starting in: {} seconds", waiting_seconds.get())}</p>
                                    </div>
                                </Show>
                            </div>
                            <div class="mb-6">
                                <h3 class="text-lg font-semibold mb-3 text-gray-700">"Players in Room:"</h3>
                                <div class="flex flex-wrap justify-center gap-3">
                                    <For
                                        each=move || players.get().into_iter().enumerate()
                                        key=|(i, p)| format!("{i}-{p}")
                                        children=move |(_idx, player)| {
                                            view! {
                                                <div class="bg-gradient-to-r from-sky-400 to-cyan-500 text-white px-4 py-2 rounded-full font-semibold shadow-lg">
                                                    {player}
                                                </div>
                                            }
                                        }
                                    />
                                </div>
                            </div>
                        </div>
                    </div>
                </Show>

                <Show when=move || game_state.get() == "finished">
                    <div class="stat-card rounded-xl shadow-xl p-6 mb-6">
                        <div class="text-center mb-6">
                            <h2 class="text-3xl font-bold text-gray-800 mb-2">"🏆 Race Complete!"</h2>
                        </div>
                        <Show when=move || (ALLOW_TEST_UI && test_mode.get())>
                            <div class="mb-4 p-3 rounded bg-yellow-100 border border-yellow-300 text-yellow-800 text-sm font-medium">"TEST MODE — Local practice (no server sync)"</div>
                        </Show>
                        <div class="grid grid-cols-1 md:grid-cols-3 gap-6 mb-6">
                            <div class="text-center p-4 bg-blue-50 rounded-lg">
                                <div class="text-4xl font-bold text-blue-600">{move || format!("{:.0}", wpm.get())}</div>
                                <div class="text-gray-600">"Words per Minute"</div>
                            </div>
                            <div class="text-center p-4 bg-green-50 rounded-lg">
                                <div class="text-4xl font-bold text-green-600">{move || format!("{:.0}%", accuracy.get())}</div>
                                <div class="text-gray-600">"Accuracy"</div>
                            </div>
                            <div class="text-center p-4 bg-purple-50 rounded-lg">
                                <div class="text-4xl font-bold text-purple-600">{move || finish_time.get().map(|t| format!("{t:.1}s")).unwrap_or_else(|| "0s".to_string())}</div>
                                <div class="text-gray-600">"Total Time"</div>
                            </div>
                        </div>
                        <Show when=move || !leaderboard.get().is_empty()>
                            <div class="mb-6">
                                <h3 class="text-xl font-semibold mb-3 text-gray-700">"Final Results:"</h3>
                                <div class="space-y-2">
                                    <For
                                        each=move || leaderboard.get().into_iter().enumerate()
                                        key=|(i, (name, _, _))| format!("{i}-{name}")
                                        children=move |(idx, (name, lwpm, lacc))| {
                                            view! { <div class="p-3 bg-gray-50 rounded-lg">{format!("#{}  {} — {:.0} WPM, {:.0}%", idx + 1, name, lwpm, lacc)}</div> }
                                        }
                                    />
                                </div>
                            </div>
                        </Show>
                        <div class="text-center">
                            <button class="bg-green-500 text-white px-8 py-3 rounded-lg hover:bg-green-600 transition-colors font-semibold text-lg"
                                on:click=move |_| {
                                    // Optimistic local reset for snappy UX
                                    set_game_state.set("waiting".to_string());
                                    set_current_position.set(0);
                                    set_errors.set(0);
                                    set_wpm.set(0.0);
                                    set_accuracy.set(100.0);
                                    set_time_elapsed.set(0.0);
                                    set_finish_time.set(None);
                                    set_leaderboard.set(Vec::new());
                                    set_player_positions.set(HashMap::new());
                                    set_test_mode.set(false);
                                    WS_REF.with(|cell| {
                                        if let Some(ws) = cell.borrow().as_ref() {
                                            let msg = ClientMsg::Reset;
                                            if let Ok(json) = serde_json::to_string(&msg) { let _ = ws.send_with_str(&json); }
                                        }
                                    });
                                }>
                                "🏁 Race Again"
                            </button>
                            <Show when=move || (ALLOW_TEST_UI && test_mode.get())>
                                <button class="ml-3 bg-gray-600 text-white px-6 py-3 rounded-lg hover:bg-gray-700 transition-colors font-semibold text-lg"
                                    on:click=move |_| {
                                        // Exit local test mode back to waiting
                                        set_game_state.set("waiting".to_string());
                                        set_current_position.set(0);
                                        set_errors.set(0);
                                        set_wpm.set(0.0);
                                        set_accuracy.set(100.0);
                                        set_time_elapsed.set(0.0);
                                        set_finish_time.set(None);
                                        set_leaderboard.set(Vec::new());
                                        set_player_positions.set(HashMap::new());
                                        set_test_mode.set(false);
                                    }>
                                    "Exit Test"
                                </button>
                            </Show>
                        </div>
                    </div>
                </Show>

                <div class="text-center text-white text-sm mt-8">
                    <p>"Built with ❤️ using Rust 🦀 + WebAssembly + WebSockets"</p>
                    <p class="mt-1">"by ystdin"</p>
                </div>
            </div>
        </div>
    }
}
