// ─── api/ws.rs ───────────────────────────────────────────────────────────────
// WebSocket state push — `/api/ws/state` mirrors `GET /api/state` but
// streams updates instead of returning a single snapshot.  Useful for
// external dashboards / live-coding editors that want to react to
// changes without polling.
//
// Protocol (V1):
//   • Client opens a WebSocket to `ws://<host>:<api_port>/api/ws/state`.
//   • Server sends the full `AppState` JSON on connect.
//   • Server then pushes a fresh full snapshot at most every
//     `WS_PUSH_INTERVAL_MS` milliseconds, but only when the snapshot
//     has actually changed since the last push (cheap pointer-style
//     comparison via the JSON serialisation length and a rolling hash
//     of the bytes — full diff out of V1 scope).
//   • Server doesn't read inbound frames; clients should ignore /
//     close.  Use the HTTP POST endpoints for writes.
//
// One task per connection.  The task drops itself on any send error
// (client closed the socket, network gone, etc.) so abandoned
// connections don't leak.

use axum::{
    extract::{
        State as AxumState,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
};
use std::time::Duration;

use super::ApiState;

/// Push interval ceiling (ms).  4 Hz is plenty for any UI dashboard
/// and keeps the serialisation cost bounded — `AppState` is ~50 KB
/// pretty-printed.  Pushes are also gated by content change, so a
/// quiet engine sends close to zero traffic.
const WS_PUSH_INTERVAL_MS: u64 = 250;

pub async fn ws_state_handler(
    AxumState(api): AxumState<ApiState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, api))
}

async fn handle_socket(mut socket: WebSocket, api: ApiState) {
    super::api_log(&api, "[API] ws/state: client connected");

    let mut last_hash: u64 = 0;
    // Initial snapshot — always send so a freshly-connected client
    // doesn't have to wait for a change to see anything.
    if !push_state(&mut socket, &api, &mut last_hash, true).await {
        return;
    }

    let mut ticker = tokio::time::interval(Duration::from_millis(WS_PUSH_INTERVAL_MS));
    // First tick fires immediately; skip it since we just sent the
    // initial snapshot.
    ticker.tick().await;
    loop {
        ticker.tick().await;
        if !push_state(&mut socket, &api, &mut last_hash, false).await {
            super::api_log(&api, "[API] ws/state: client disconnected");
            return;
        }
    }
}

/// Serialise the current AppState and send it iff the content has
/// changed since `last_hash` (or `force` is true for the initial
/// push).  Returns `false` on send error so the caller can drop the
/// connection.
async fn push_state(
    socket: &mut WebSocket,
    api: &ApiState,
    last_hash: &mut u64,
    force: bool,
) -> bool {
    let snapshot = api.app_state.read().clone();
    let json = match serde_json::to_string(&snapshot) {
        Ok(s) => s,
        Err(_) => return true, // serialisation failures shouldn't kill the socket
    };
    let h = ws_state_hash(json.as_bytes());
    if !force && h == *last_hash {
        return true; // unchanged — skip the send to keep traffic low
    }
    *last_hash = h;
    socket.send(Message::Text(json)).await.is_ok()
}

/// Cheap rolling hash over the serialised state bytes.  FNV-1a 64-bit;
/// collision probability is fine for "did anything change" detection
/// at our update rate.  Pulled out so tests can verify identity vs.
/// inequality without spinning up a socket.
pub fn ws_state_hash(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}
