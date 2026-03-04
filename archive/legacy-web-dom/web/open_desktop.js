// Optional helper to hand off to the desktop app from the browser build.
// Usage: window.NEARx?.openInDesktop('v1/tx/ABC123')
;(() => {
  if (!window.NEARx) window.NEARx = {};
  window.NEARx.openInDesktop = function(routeOrUrl) {
    try {
      let url = String(routeOrUrl || '');
      if (!/^nearx:\/\//i.test(url)) {
        url = 'nearx://' + url.replace(/^\/+/, '');
      }
      // In a regular browser this will invoke the OS protocol handler (NEARx desktop).
      // In Tauri we don't need this; the deep-link plugin emits events directly.
      window.location.href = url;
      return true;
    } catch (_e) {
      return false;
    }
  };
})();
