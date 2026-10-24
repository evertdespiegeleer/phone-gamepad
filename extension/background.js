// Holds the single connection to the phone-gamepad server and fans updates out to every frame.
// The connection lives here rather than in the page so page CSPs and local-network prompts don't apply.

// Matches the CLI's --bridge-port: the default here, the actual one in a copy written by `phone-gamepad extension`.
const server = 'localhost:47800';

const ports = new Set();
// player -> last update, replayed to frames that connect later.
const players = new Map();

function broadcast(msg) {
  for (const port of ports) port.postMessage(msg);
}

async function connect() {
  // The server is usually not running. A failed WebSocket always logs an error on chrome://extensions, a caught fetch failure doesn't, so check it's up first.
  try {
    await fetch(`http://${server}/`, { mode: 'no-cors' });
  } catch {
    setTimeout(connect, 2000);
    return;
  }

  const ws = new WebSocket(`ws://${server}/bridge`);
  ws.onmessage = (e) => {
    const update = JSON.parse(e.data);
    if (update.extension != null) {
      // Only a copy written by the CLI has a version_name. The CLI sends its fingerprint after rewriting that copy, so a reload picks up the new files.
      const { version_name } = chrome.runtime.getManifest();
      if (version_name != null && update.extension !== version_name) chrome.runtime.reload();
      return;
    }
    if (update.p == null) return; // heartbeat
    if (update.b == null) players.delete(update.p);
    else players.set(update.p, update);
    broadcast(update);
  };
  ws.onclose = () => {
    for (const p of players.keys()) broadcast({ p });
    players.clear();
    setTimeout(connect, 2000);
  };
}

chrome.runtime.onConnect.addListener((port) => {
  ports.add(port);
  for (const update of players.values()) port.postMessage(update);
  port.onDisconnect.addListener(() => ports.delete(port));
});

connect();
