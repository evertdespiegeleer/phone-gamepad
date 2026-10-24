// Runs in the page's JS world before its own scripts, adding phone gamepads to navigator.getGamepads().

(() => {
  const realGetGamepads = Navigator.prototype.getGamepads;
  // player -> Gamepad-like object with the standard mapping (17 buttons, 4 axes).
  const pads = new Map();

  function getGamepads() {
    const list = [...realGetGamepads.call(this)];
    for (const pad of pads.values()) list[pad.index] = pad;
    return list;
  }

  function freeIndex() {
    const taken = [...realGetGamepads.call(navigator)];
    for (const pad of pads.values()) taken[pad.index] = pad;
    const index = taken.findIndex((pad) => pad == null);
    return index === -1 ? taken.length : index;
  }

  function fire(type, pad) {
    // GamepadEvent rejects objects that aren't real Gamepads, so attach it to a plain Event.
    const event = new Event(type);
    Object.defineProperty(event, 'gamepad', { value: pad });
    window.dispatchEvent(event);
  }

  Object.defineProperty(Navigator.prototype, 'getGamepads', { value: getGamepads, configurable: true, writable: true });

  document.addEventListener('phone-gamepad', (e) => {
    const { p, b, a } = JSON.parse(e.detail);
    let pad = pads.get(p);

    if (b == null) {
      if (pad == null) return;
      pads.delete(p);
      pad.connected = false;
      fire('gamepaddisconnected', pad);
      return;
    }

    const isNew = pad == null;
    if (isNew) {
      pad = {
        id: `Phone Gamepad ${p} (STANDARD GAMEPAD)`,
        index: freeIndex(),
        connected: true,
        mapping: 'standard',
        timestamp: 0,
        axes: [0, 0, 0, 0],
        buttons: Array.from({ length: 17 }, () => ({ pressed: false, touched: false, value: 0 })),
        vibrationActuator: null,
      };
      pads.set(p, pad);
    }

    pad.timestamp = performance.now();
    pad.axes = a;
    pad.buttons = pad.buttons.map((_, i) => {
      const pressed = ((b >> i) & 1) === 1;
      return { pressed, touched: pressed, value: pressed ? 1 : 0 };
    });

    if (isNew) fire('gamepadconnected', pad);
  });
})();
