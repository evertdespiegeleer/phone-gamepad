use axum::Router;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Html;
use axum::routing::get;
use clap::Parser;
use qrcode::QrCode;
use qrcode::render::unicode::Dense1x2;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{IsTerminal, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;

/// Turn phones into gamepads for browser games on this machine.
///
/// Serves a gamepad page for phones and prints a QR code to open it. The Phone Gamepad Chrome extension connects to the bridge port and exposes each phone to web pages through the Gamepad API.
#[derive(Parser)]
#[command(version)]
struct Args {
    /// Port for the phone page. Defaults to the first free port from 47801 up.
    #[arg(short, long, env = "PHONE_GAMEPAD_PORT")]
    port: Option<u16>,

    /// Hostname or IP phones use to reach this machine, e.g. a Tailscale MagicDNS name. Defaults to this machine's LAN IP.
    #[arg(long, env = "PHONE_GAMEPAD_HOST")]
    host: Option<String>,

    /// Localhost port the Chrome extension connects to. Must match `server` in the extension.
    #[arg(long, env = "PHONE_GAMEPAD_BRIDGE_PORT", default_value_t = 47800)]
    bridge_port: u16,
}

/// State sent by a phone: button bitmask and left X/Y, right X/Y axes in -1.0..=1.0.
#[derive(Clone, Default, Deserialize, Serialize)]
struct Input {
    b: u16,
    a: [f32; 4],
}

/// Message relayed to the extension. A missing `input` means the player disconnected.
#[derive(Serialize)]
struct Update {
    p: usize,
    #[serde(flatten)]
    input: Option<Input>,
}

/// Player colors by player number: the classic ones first, more exotic ones for bigger groups. Repeats after the last one.
const PLAYER_COLORS: [(u8, u8, u8); 12] = [
    (0xff, 0x3b, 0x30), // red
    (0xff, 0xd6, 0x0a), // yellow
    (0x30, 0xd1, 0x58), // green
    (0x0a, 0x84, 0xff), // blue
    (0xbf, 0x5a, 0xf2), // purple
    (0xff, 0x9f, 0x0a), // orange
    (0xff, 0x6e, 0xb4), // pink
    (0x64, 0xd2, 0xff), // cyan
    (0xb4, 0xf0, 0x00), // lime
    (0xa2, 0x84, 0x5e), // brown
    (0x00, 0xc7, 0xbe), // teal
    (0xf2, 0xf2, 0xf7), // white
];

fn player_color(p: usize) -> (u8, u8, u8) {
    PLAYER_COLORS[(p - 1) % PLAYER_COLORS.len()]
}

#[derive(Clone)]
struct AppState {
    players: Arc<Mutex<HashMap<usize, Input>>>,
    updates: broadcast::Sender<String>,
    status: Arc<Mutex<Status>>,
}

/// The live status block printed below the QR code.
#[derive(Default)]
struct Status {
    /// The QR code and info lines above the status, reprinted when the terminal is resized.
    header: String,
    browsers: usize,
    last_error: Option<String>,
    /// Lines printed by the previous render, to move the cursor back over.
    printed: usize,
}

impl AppState {
    /// Claims the lowest free player number, so numbers of disconnected players are reused.
    fn join(&self) -> usize {
        let mut players = self.players.lock().unwrap();
        let p = (1..).find(|p| !players.contains_key(p)).unwrap();
        players.insert(p, Input::default());
        self.send(p, Some(Input::default()));
        p
    }

    fn publish(&self, p: usize, input: Option<Input>) {
        let mut players = self.players.lock().unwrap();
        match &input {
            Some(input) => players.insert(p, input.clone()),
            None => players.remove(&p),
        };
        self.send(p, input);
    }

    fn send(&self, p: usize, input: Option<Input>) {
        // Sending fails only when no extension is connected, which is fine.
        let _ = self.updates.send(serde_json::to_string(&Update { p, input }).unwrap());
    }

    fn update_status(&self, change: impl FnOnce(&mut Status)) {
        self.render(change, false);
    }

    /// Redraws the whole screen. Terminals reflow text on resize, so redrawing only the status block wouldn't land in the right place.
    fn redraw(&self) {
        self.render(|_| {}, true);
    }

    fn render(&self, change: impl FnOnce(&mut Status), full: bool) {
        let mut players: Vec<usize> = self.players.lock().unwrap().keys().copied().collect();
        players.sort();
        let mut status = self.status.lock().unwrap();
        change(&mut status);

        let mut out = std::io::stdout().lock();
        let terminal = out.is_terminal();

        let dot = |p: usize| {
            let (r, g, b) = player_color(p);
            if terminal { format!("\x1b[38;2;{r};{g};{b}m●\x1b[0m") } else { "●".to_string() }
        };
        let players = if players.is_empty() { "none".to_string() } else { players.iter().map(|&p| format!("{} {p}", dot(p))).collect::<Vec<_>>().join(", ") };
        let browser = match status.browsers {
            0 => "not connected (is the extension loaded?)".to_string(),
            1 => "connected".to_string(),
            n => format!("connected ({n} browsers)"),
        };
        let mut lines = vec![format!("Players: {players}"), format!("Browser: {browser}")];
        if let Some(err) = &status.last_error {
            lines.push(format!("Last error: {err}"));
        }

        // Redraw in place on a terminal; elsewhere (e.g. piped to a file) just append.
        if terminal {
            // Redrawing in place counts lines, so cut them to the terminal width to keep them from wrapping. Colors are dropped from a line that needs cutting.
            let width = terminal_size::terminal_size().map_or(usize::MAX, |(width, _)| width.0 as usize);
            for line in &mut lines {
                let visible = strip_ansi(line);
                if visible.chars().count() >= width {
                    *line = visible.chars().take(width.saturating_sub(1)).collect();
                }
            }
            if full {
                write!(out, "\x1b[H\x1b[2J\x1b[3J{}", status.header).unwrap();
            } else if status.printed > 0 {
                write!(out, "\x1b[{}A\x1b[J", status.printed).unwrap();
            }
        } else if full {
            return;
        }
        writeln!(out, "{}", lines.join("\n")).unwrap();
        out.flush().unwrap();
        status.printed = lines.len();
    }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let state = AppState { players: Default::default(), updates: broadcast::channel(256).0, status: Default::default() };

    let phone_app = Router::new()
        .route("/", get(|| async { Html(include_str!("../static/index.html")) }))
        .route("/ws", get(|upgrade: WebSocketUpgrade, State(state): State<AppState>| async move { upgrade.on_upgrade(|socket| phone(socket, state)) }))
        .with_state(state.clone());
    let bridge_app = Router::new()
        // Lets the extension check the server is up without a WebSocket attempt, which would log an error in Chrome when it fails.
        .route("/", get(|| async { "phone-gamepad bridge" }))
        .route("/bridge", get(|upgrade: WebSocketUpgrade, State(state): State<AppState>| async move { upgrade.on_upgrade(|socket| bridge(socket, state)) }))
        .with_state(state.clone());

    let phone_listener = match args.port {
        Some(port) => bind(("0.0.0.0", port), "phone port").await,
        None => first_free_port(DEFAULT_PORT).await,
    };
    // The bridge only serves the local browser, so it isn't exposed on the network.
    let bridge_listener = bind(("127.0.0.1", args.bridge_port), "bridge port (is another phone-gamepad running? see --bridge-port)").await;

    let host = args.host.unwrap_or_else(|| local_ip_address::local_ip().expect("no local network address found").to_string());
    let url = format!("http://{host}:{}", phone_listener.local_addr().unwrap().port());
    let qr = QrCode::new(&url).expect("failed to encode QR code");
    let header = format!(
        "{}\nScan to open {url}\nThis is plain http. If the phone tries https instead, turn off its browser's HTTPS-only / secure-connections setting or choose to continue to the site.\nBrowser extension bridge on ws://localhost:{}/bridge\n\n",
        qr.render::<Dense1x2>().quiet_zone(true).build(),
        args.bridge_port,
    );
    print!("{header}");
    state.update_status(|status| status.header = header);

    // Windows has no resize signal; the status there is redrawn on the next change instead.
    #[cfg(unix)]
    let resize_state = state.clone();
    #[cfg(unix)]
    tokio::spawn(async move {
        let Ok(mut resized) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::window_change()) else { return };
        while resized.recv().await.is_some() {
            resize_state.redraw();
        }
    });

    let (phone_result, bridge_result) = tokio::join!(axum::serve(phone_listener, phone_app), axum::serve(bridge_listener, bridge_app));
    phone_result.unwrap();
    bridge_result.unwrap();
}

fn strip_ansi(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip a CSI sequence: ESC [ ... final letter.
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

// Well-known ports like 8080 are often intercepted by proxies and network filters; high ports get through more reliably.
const DEFAULT_PORT: u16 = 47801;

/// Binds the first port from `start` up that isn't in use.
async fn first_free_port(start: u16) -> tokio::net::TcpListener {
    for port in start..=u16::MAX {
        match tokio::net::TcpListener::bind(("0.0.0.0", port)).await {
            Ok(listener) => return listener,
            Err(err) if err.kind() == std::io::ErrorKind::AddrInUse => continue,
            Err(err) => {
                eprintln!("failed to bind phone port {port}: {err}");
                std::process::exit(1);
            }
        }
    }
    eprintln!("no free phone port from {start} up");
    std::process::exit(1);
}

async fn bind(addr: (&str, u16), what: &str) -> tokio::net::TcpListener {
    tokio::net::TcpListener::bind(addr).await.unwrap_or_else(|err| {
        eprintln!("failed to bind {what}: {err}");
        std::process::exit(1);
    })
}

/// A phone connection. Each phone becomes its own player.
async fn phone(mut socket: WebSocket, state: AppState) {
    let p = state.join();
    state.update_status(|_| {});
    let (r, g, b) = player_color(p);
    let hello = serde_json::json!({ "player": p, "color": format!("#{r:02x}{g:02x}{b:02x}") });
    let _ = socket.send(Message::Text(hello.to_string().into())).await;

    while let Some(Ok(msg)) = socket.recv().await {
        let Message::Text(text) = msg else { continue };
        match serde_json::from_str::<Input>(&text) {
            Ok(input) => state.publish(p, Some(input)),
            Err(err) => state.update_status(|status| status.last_error = Some(format!("player {p} sent a bad message: {err}"))),
        }
    }

    state.publish(p, None);
    state.update_status(|_| {});
}

/// A browser extension connection: gets the current players, then every update.
async fn bridge(mut socket: WebSocket, state: AppState) {
    let mut updates = state.updates.subscribe();
    let snapshot: Vec<String> = state.players.lock().unwrap().iter().map(|(&p, input)| serde_json::to_string(&Update { p, input: Some(input.clone()) }).unwrap()).collect();
    for msg in snapshot {
        if socket.send(Message::Text(msg.into())).await.is_err() {
            return;
        }
    }
    state.update_status(|status| status.browsers += 1);

    // Chrome only keeps the extension's service worker alive while the socket sees traffic.
    let mut heartbeat = tokio::time::interval(Duration::from_secs(20));
    loop {
        let msg = tokio::select! {
            update = updates.recv() => match update {
                Ok(update) => update,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            },
            _ = heartbeat.tick() => "{}".to_string(),
            incoming = socket.recv() => match incoming {
                Some(Ok(_)) => continue,
                _ => break,
            },
        };
        if socket.send(Message::Text(msg.into())).await.is_err() {
            break;
        }
    }
    state.update_status(|status| status.browsers -= 1);
}
