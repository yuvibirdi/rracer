use leptos::*;
use shared::protocol::{ClientMsg, ServerMsg};
use std::collections::HashMap;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use web_sys::{MessageEvent, WebSocket};

use crate::websocket::WebSocketManager;

#[component]
pub fn App() -> impl IntoView {
    let (game_state, set_game_state) = create_signal("waiting".to_string());
    let (players, set_players) = create_signal(Vec::<String>::new());
    let (passage, set_passage) = create_signal(String::new());
    let (player_positions, set_player_positions) = create_signal(HashMap::<String, usize>::new());
    let (current_position, set_current_position) = create_signal(0usize);
    let (errors, set_errors) = create_signal(0usize);
    let (start_time, set_start_time) = create_signal(None::<f64>);
    let (room_name, set_room_name) = create_signal("main".to_string());
    let (player_name, set_player_name) = create_signal("Player".to_string());
    let (ws_manager, set_ws_manager) = create_signal(None::<WebSocketManager>);
    let (connected, set_connected) = create_signal(false);

    let connect_websocket = move || {
        let host = web_sys::window()
            .unwrap()
            .location()
            .host()
            .unwrap();
        let ws_url = format!("ws://{}/ws", host);
        
        match WebSocketManager::new(&ws_url) {
            Ok(manager) => {
                let manager_clone = manager.clone();
                
                // Set up message handler
                manager.set_message_handler(move |msg: ServerMsg| {
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
                        ServerMsg::Finish { id, wpm, accuracy } => {
                            web_sys::console::log_1(&format!("Player {} finished with {} WPM, {}% accuracy", id, wpm, accuracy).into());
                        }
                        ServerMsg::StateChange { state } => {
                            set_game_state.set(state);
                        }
                        ServerMsg::Error { message } => {
                            web_sys::console::error_1(&message.into());
                        }
                    }
                });
                
                set_ws_manager.set(Some(manager_clone));
                set_connected.set(true);
            }
            Err(e) => {
                web_sys::console::error_1(&format!("Failed to connect: {}", e).into());
            }
        }
    };

    let join_room = move || {
        if let Some(ws) = ws_manager.get() {
            let msg = ClientMsg::Join {
                room: room_name.get(),
                name: player_name.get(),
            };
            ws.send_message(msg);
        }
    };

    let handle_keydown = move |ev: web_sys::KeyboardEvent| {
        if game_state.get() != "racing" {
            return;
        }

        let key = ev.key();
        if let Some(ch) = key.chars().next() {
            let passage_text = passage.get();
            if let Some(expected_char) = passage_text.chars().nth(current_position.get()) {
                if ch == expected_char {
                    set_current_position.update(|pos| *pos += 1);
                    
                    if let Some(ws) = ws_manager.get() {
                        let msg = ClientMsg::Key {
                            ch,
                            ts: js_sys::Date::now() as u64,
                        };
                        ws.send_message(msg);
                    }
                } else {
                    set_errors.update(|e| *e += 1);
                }
            }
        }
    };

    let reset_game = move || {
        if let Some(ws) = ws_manager.get() {
            ws.send_message(ClientMsg::Reset);
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
                        <h2 class="text-xl font-semibold mb-4">"Type this passage:"</h2>
                        <div 
                            class="text-lg font-mono leading-relaxed p-4 bg-gray-50 rounded border-2 border-gray-200 focus-within:border-blue-500"
                            tabindex="0"
                            on:keydown=handle_keydown
                        >
                            <span class="bg-green-200">{move || passage.get().chars().take(current_position.get()).collect::<String>()}</span>
                            <span class="bg-blue-200">{move || passage.get().chars().nth(current_position.get()).unwrap_or(' ')}</span>
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
                            on:click=move |_| reset_game()
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
