use leptos::prelude::*;
use shared::protocol::{ClientMsg, ServerMsg};
use std::collections::HashMap;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::WebSocket;
use std::cell::RefCell;

// Thread-local storage for the active WebSocket. This avoids capturing non-Send/Sync
// types inside Leptos children closures, which require Fn + Send + Sync.
thread_local! {
    static WS_REF: RefCell<Option<WebSocket>> = RefCell::new(None);
}

#[component]
pub fn App() -> impl IntoView {
    let (game_state, set_game_state) = signal("waiting".to_string());
    let (players, set_players) = signal(Vec::<String>::new());
    let (passage, set_passage) = signal(String::new());
    let (player_positions, set_player_positions) = signal(HashMap::<String, usize>::new());
    let (current_position, set_current_position) = signal(0usize);
    let (errors, set_errors) = signal(0usize);
    let (start_time, set_start_time) = signal(None::<f64>);
    let (room_name, set_room_name) = signal("main".to_string());
    let (player_name, set_player_name) = signal("Player".to_string());
    let (connected, set_connected) = signal(false);
    let (error_message, set_error_message) = signal(None::<String>);
    let (wpm, set_wpm) = signal(0.0);
    let (accuracy, set_accuracy) = signal(100.0);
    
    // WebSocket is managed via thread-local storage (WS_REF)

    let connect_websocket = {
        move || {
            let host = web_sys::window()
                .unwrap()
                .location()
                .host()
                .unwrap();
            let ws_url = format!("ws://{}/ws", host);
            
            match WebSocket::new(&ws_url) {
                Ok(ws) => {
                    // Set up message handler
                    let onmessage_callback = {
                        let set_players = set_players.clone();
                        let set_passage = set_passage.clone();
                        let set_game_state = set_game_state.clone();
                        let set_start_time = set_start_time.clone();
                        let set_current_position = set_current_position.clone();
                        let set_errors = set_errors.clone();
                        let set_player_positions = set_player_positions.clone();
                        let set_wpm = set_wpm.clone();
                        let set_accuracy = set_accuracy.clone();
                        let set_error_message = set_error_message.clone();
                        
                        Closure::wrap(Box::new(move |e: web_sys::MessageEvent| {
                            if let Ok(text) = e.data().dyn_into::<js_sys::JsString>() {
                                let text: String = text.into();
                                if let Ok(msg) = serde_json::from_str::<ServerMsg>(&text) {
                                    match msg {
                                        ServerMsg::Lobby { players: p } => {
                                            set_players.set(p);
                                        }
                                        ServerMsg::Start { passage: p, t0: _ } => {
                                            set_passage.set(p);
                                            set_game_state.set("racing".to_string());
                                            set_start_time.set(Some(js_sys::Date::now()));
                                            set_current_position.set(0);
                                            set_errors.set(0);
                                        }
                                        ServerMsg::Progress { id, pos } => {
                                            set_player_positions.update(|positions| {
                                                positions.insert(id, pos);
                                            });
                                        }
                                        ServerMsg::Finish { id, wpm: player_wpm, accuracy: player_accuracy } => {
                                            web_sys::console::log_1(&format!("Player {} finished with {} WPM, {}% accuracy", id, player_wpm, player_accuracy).into());
                                            set_wpm.set(player_wpm);
                                            set_accuracy.set(player_accuracy);
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
                                            }
                                        }
                                         ServerMsg::WaitingTimer { seconds_left } => {
                                             // Optional: could expose a signal to show countdown in UI.
                                             web_sys::console::log_1(&format!("Waiting... {}s", seconds_left).into());
                                         }
                                        ServerMsg::Error { message } => {
                                            set_error_message.set(Some(message.clone()));
                                            web_sys::console::error_1(&message.into());
                                        }
                                    }
                                }
                            }
                        }) as Box<dyn FnMut(_)>)
                    };
                    
                    ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
                    onmessage_callback.forget();
                    
                    WS_REF.with(|cell| {
                        *cell.borrow_mut() = Some(ws);
                    });
                    set_connected.set(true);
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
                }
            });
        }
    };

    view! {
        <div class="min-h-screen bg-gray-100 p-8">
            <div class="max-w-4xl mx-auto">
                <h1 class="text-4xl font-bold text-center mb-8 text-blue-600">"🏁 rracer"</h1>
                
                <div class="bg-white rounded-lg shadow-lg p-6 mb-6">
                    <div class="flex gap-4 mb-4">
                        <input
                            type="text"
                            placeholder="Room name"
                            class="border rounded px-3 py-2 flex-1"
                            prop:value=room_name
                            on:input=move |ev| set_room_name.set(event_target_value(&ev))
                        />
                        <input
                            type="text"
                            placeholder="Your name"
                            class="border rounded px-3 py-2 flex-1"
                            prop:value=player_name
                            on:input=move |ev| set_player_name.set(event_target_value(&ev))
                        />
                        <button
                            class="bg-blue-500 text-white px-4 py-2 rounded hover:bg-blue-600"
                            on:click=move |_| {
                                if !connected.get() {
                                    connect_websocket();
                                }
                                join_room();
                            }
                        >
                            {move || if connected.get() { "Join Room" } else { "Connect & Join" }}
                        </button>
                    </div>
                    
                    <div class="text-sm text-gray-600">
                        "Status: " <span class="font-semibold">{game_state}</span>
                        {move || if connected.get() { " • Connected" } else { " • Disconnected" }}
                    </div>
                </div>

                <Show when=move || !players.get().is_empty()>
                    <div class="bg-white rounded-lg shadow-lg p-6 mb-6">
                        <h2 class="text-xl font-semibold mb-4">"Players in Room"</h2>
                        <div class="flex flex-wrap gap-2">
                            <For
                                each=move || players.get().into_iter().enumerate()
                                key=|(i, _)| *i
                                children=move |(_, player)| {
                                    let pos = player_positions.get().get(&player).copied().unwrap_or(0);
                                    view! {
                                        <div class="bg-gray-100 rounded px-3 py-1">
                                            {player} " (" {pos} ")"
                                        </div>
                                    }
                                }
                            />
                        </div>
                    </div>
                </Show>

                <Show when=move || !passage.get().is_empty()>
                    <div class="bg-white rounded-lg shadow-lg p-6 mb-6">
                        <div class="flex justify-between items-center mb-4">
                            <h2 class="text-xl font-semibold">"Type this passage:"</h2>
                            <div class="flex gap-4 text-sm">
                                <div class="text-center">
                                    <div class="font-bold text-lg text-blue-600">{move || format!("{:.0}", wpm.get())}</div>
                                    <div class="text-gray-500">"WPM"</div>
                                </div>
                                <div class="text-center">
                                    <div class="font-bold text-lg text-green-600">{move || format!("{:.1}%", accuracy.get())}</div>
                                    <div class="text-gray-500">"Accuracy"</div>
                                </div>
                            </div>
                        </div>
                        
                        <Show when=move || error_message.get().is_some()>
                            <div class="mb-4 p-3 bg-red-100 border border-red-400 text-red-700 rounded">
                                {move || error_message.get().unwrap_or_default()}
                            </div>
                        </Show>
                        
                        <div 
                            class="text-lg font-mono leading-relaxed p-4 bg-gray-50 rounded border-2 border-gray-200 focus-within:border-blue-500 typing-area"
                            tabindex="0"
                            on:keydown=move |ev: web_sys::KeyboardEvent| {
                                if game_state.get() != "racing" {
                                    return;
                                }

                                let key = ev.key();
                                if let Some(ch) = key.chars().next() {
                                    let passage_text = passage.get();
                                    if let Some(expected_char) = passage_text.chars().nth(current_position.get()) {
                                        if ch == expected_char {
                                            set_current_position.update(|pos| *pos += 1);

                                            // Calculate real-time WPM
                                            if let Some(start) = start_time.get() {
                                                let elapsed = (js_sys::Date::now() - start) / 1000.0; // seconds
                                                if elapsed > 0.0 {
                                                    let chars_typed = current_position.get() + 1;
                                                    let gross_wpm = (chars_typed as f64 / 5.0) / (elapsed / 60.0);
                                                    let net_wpm = gross_wpm - (errors.get() as f64 * 60.0 / elapsed);
                                                    set_wpm.set(net_wpm.max(0.0));

                                                    let total_chars = chars_typed + errors.get();
                                                    if total_chars > 0 {
                                                        set_accuracy.set((chars_typed as f64 / total_chars as f64) * 100.0);
                                                    }
                                                }
                                            }

                                            WS_REF.with(|cell| {
                                                if let Some(ws) = cell.borrow().as_ref() {
                                                    let msg = ClientMsg::Key {
                                                        ch,
                                                        ts: js_sys::Date::now() as u64,
                                                    };
                                                    if let Ok(json) = serde_json::to_string(&msg) {
                                                        let _ = ws.send_with_str(&json);
                                                    }
                                                }
                                            });
                                        } else {
                                            set_errors.update(|e| *e += 1);

                                            // Update accuracy on error
                                            let total_chars = current_position.get() + errors.get();
                                            if total_chars > 0 {
                                                set_accuracy.set((current_position.get() as f64 / total_chars as f64) * 100.0);
                                            }
                                        }
                                    }
                                }
                            }
                        >
                            <span class="correct-char">{move || passage.get().chars().take(current_position.get()).collect::<String>()}</span>
                            <span class="current-char">{move || passage.get().chars().nth(current_position.get()).unwrap_or(' ')}</span>
                            <span>{move || passage.get().chars().skip(current_position.get() + 1).collect::<String>()}</span>
                        </div>
                        <div class="mt-4 flex justify-between text-sm text-gray-600">
                            <span>"Position: " {current_position} " / " {move || passage.get().len()}</span>
                            <span>"Errors: " {errors}</span>
                        </div>
                    </div>
                </Show>

                <Show when=move || game_state.get() == "finished">
                    <div class="bg-white rounded-lg shadow-lg p-6">
                        <h2 class="text-xl font-semibold mb-4">"Race Finished!"</h2>
                        <button
                            class="bg-green-500 text-white px-4 py-2 rounded hover:bg-green-600"
                            on:click=move |_| {
                                WS_REF.with(|cell| {
                                    if let Some(ws) = cell.borrow().as_ref() {
                                        let msg = ClientMsg::Reset;
                                        if let Ok(json) = serde_json::to_string(&msg) {
                                            let _ = ws.send_with_str(&json);
                                        }
                                    }
                                });
                            }
                        >
                            "Start New Race"
                        </button>
                    </div>
                </Show>

                <div class="text-center text-sm text-gray-500 mt-8">
                    "Built with Rust 🦀 + WebAssembly + Leptos"
                </div>
            </div>
        </div>
    }
}
