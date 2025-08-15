use shared::protocol::{ClientMsg, ServerMsg};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{MessageEvent, WebSocket};

// This optional helper isn't wired into the app yet.
// Allow dead_code to keep the file around without warnings until it's integrated.
#[allow(dead_code)]
#[derive(Clone)]
pub struct WebSocketManager {
    ws: Rc<WebSocket>,
    message_handler: Rc<RefCell<Option<Box<dyn Fn(ServerMsg)>>>>,
}

#[allow(dead_code)]
impl WebSocketManager {
    pub fn new(url: &str) -> Result<Self, JsValue> {
        let ws = WebSocket::new(url)?;
        let ws = Rc::new(ws);
        let message_handler = Rc::new(RefCell::new(None));

        let manager = Self {
            ws: ws.clone(),
            message_handler: message_handler.clone(),
        };

        // Set up event handlers
    let _ws_clone = ws.clone();
        let message_handler_clone = message_handler.clone();
        
        let onmessage_callback = Closure::wrap(Box::new(move |e: MessageEvent| {
            if let Ok(text) = e.data().dyn_into::<js_sys::JsString>() {
                let text: String = text.into();
                if let Ok(msg) = serde_json::from_str::<ServerMsg>(&text) {
                    if let Some(handler) = message_handler_clone.borrow().as_ref() {
                        handler(msg);
                    }
                }
            }
        }) as Box<dyn FnMut(_)>);
        
        ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
        onmessage_callback.forget();

        let onopen_callback = Closure::wrap(Box::new(move |_: web_sys::Event| {
            web_sys::console::log_1(&"WebSocket connected".into());
        }) as Box<dyn FnMut(_)>);
        
        ws.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
        onopen_callback.forget();

        let onerror_callback = Closure::wrap(Box::new(move |_: web_sys::Event| {
            web_sys::console::error_1(&"WebSocket error".into());
        }) as Box<dyn FnMut(_)>);
        
        ws.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));
        onerror_callback.forget();

        let onclose_callback = Closure::wrap(Box::new(move |_: web_sys::CloseEvent| {
            web_sys::console::log_1(&"WebSocket disconnected".into());
        }) as Box<dyn FnMut(_)>);
        
        ws.set_onclose(Some(onclose_callback.as_ref().unchecked_ref()));
        onclose_callback.forget();

        Ok(manager)
    }

    #[allow(dead_code)]
    pub fn set_message_handler<F>(&self, handler: F)
    where
        F: Fn(ServerMsg) + 'static,
    {
        *self.message_handler.borrow_mut() = Some(Box::new(handler));
    }

    #[allow(dead_code)]
    pub fn send_message(&self, msg: ClientMsg) {
        if let Ok(json) = serde_json::to_string(&msg) {
            let _ = self.ws.send_with_str(&json);
        }
    }
}
