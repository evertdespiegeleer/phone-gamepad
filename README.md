# phone-gamepad

Use your phone as a gamepad for browser games. A small CLI serves a gamepad page and prints a QR code to open it on your phone; a Chrome extension makes each connected phone show up to web pages as a regular gamepad.

> [!NOTE]
> This is a little tool I built for myself because of some experiments I'm doing. I don't necessarily intend to maintain it, so issues and PRs may go unanswered. If you find it useful, a star is appreciated.

## Features

- Phone layouts: full (two sticks, d-pad, ABXY, four shoulder buttons), retro, single stick and twin stick, in portrait or landscape.
- Per-axis stick behavior: spring back to center, stay put, or stay put starting at the end (e.g. an RC throttle that starts at zero).
- Multiple phones at once, each as its own player. A freed player number goes to the next phone that joins.
- No drivers, no root, no system settings: works in any page that uses the standard Gamepad API.

## Install

Requires a [Rust toolchain](https://rustup.rs) and a Chromium-based browser (Chrome, Edge, Brave, Arc, …).

1. Install the CLI:
   ```sh
   git clone git@github.com:evertdespiegeleer/phone-gamepad.git && cd phone-gamepad
   cargo install --path .
   ```
2. Load the extension in Chrome:
   1. Open `chrome://extensions`.
   2. Turn on **Developer mode** (top right).
   3. Click **Load unpacked** and select the `extension/` directory of this repo.

The extension stays installed across browser restarts. After pulling changes to `extension/`, click the reload icon on its card in `chrome://extensions`.

## Usage

```sh
phone-gamepad
```

Scan the QR code with a phone on the same network, then open a browser game. Tap the gear on the phone to change layout and stick behavior; the choice is remembered on that phone.

To check that it works, open a gamepad tester such as [hardwaretester.com/gamepad](https://hardwaretester.com/gamepad) in Chrome and move a stick on the phone.

### Options

| Flag | Env | Default | Description |
|---|---|---|---|
| `-p, --port` | `PHONE_GAMEPAD_PORT` | first free port from 47801 | Port for the phone page |
| `--host` | `PHONE_GAMEPAD_HOST` | LAN IP | Hostname or IP in the QR code |
| `--bridge-port` | `PHONE_GAMEPAD_BRIDGE_PORT` | `47800` | Localhost-only port the extension connects to |

Use `--host` when the phone should reach the machine by another name, e.g. its Tailscale MagicDNS name: `phone-gamepad --host my-macbook`.

The extension expects the default bridge port. To change it, also change `server` in `extension/background.js` and reload the extension.

## How it works

- Phones send their state over a WebSocket to `/ws` on the phone port; the CLI relays it to the extension on `/bridge` on the bridge port.
- The extension's service worker holds that connection, so page CSPs don't apply, and forwards updates to every frame.
- `page.js` runs before the page's own scripts and adds the phone gamepads to `navigator.getGamepads()`, firing `gamepadconnected` / `gamepaddisconnected`.

Gamepads use the [standard mapping](https://w3c.github.io/gamepad/#remapping): 17 buttons (A, B, X, Y, L1, R1, L2, R2, Select, Start, L3, R3, d-pad, Home; L3, R3 and Home are never pressed) and 4 axes (left X/Y, right X/Y).

## Limitations

- Only browser games in Chromium-based browsers; native games don't see the phone. A system-wide virtual gamepad isn't an option on macOS: creating virtual HID devices requires Apple's restricted `com.apple.developer.hid.virtual.device` entitlement, and root no longer bypasses it.
- The phone and the machine need to reach each other directly. VPNs and endpoint security software on the host can slow down or drop the phone's connection; the page retries automatically.
