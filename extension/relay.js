// Forwards updates from the service worker to page.js, which runs in the page's own JS world.

function connect() {
  const port = chrome.runtime.connect();
  port.onMessage.addListener((update) => {
    document.dispatchEvent(new CustomEvent('phone-gamepad', { detail: JSON.stringify(update) }));
  });
  // The service worker can be restarted by Chrome; reconnect when that drops the port.
  port.onDisconnect.addListener(() => setTimeout(connect, 1000));
}

connect();
