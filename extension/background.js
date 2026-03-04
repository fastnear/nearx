const NATIVE_HOST = "com.nearx.native";

function openDeepLinkViaNative(url) {
  return new Promise((resolve) => {
    let settled = false;
    let timeoutId = null;
    let port = null;

    const finish = (result) => {
      if (settled) return;
      settled = true;
      if (timeoutId) clearTimeout(timeoutId);
      try {
        if (port) port.disconnect();
      } catch {}
      resolve(result);
    };

    try {
      port = chrome.runtime.connectNative(NATIVE_HOST);
    } catch (err) {
      finish({ ok: false, message: String(err || "native host connect failed") });
      return;
    }

    timeoutId = setTimeout(() => {
      finish({ ok: false, message: "native host timeout" });
    }, 5000);

    port.onMessage.addListener((resp) => {
      // Host sends an initial hello frame; wait for operation result.
      if (resp && resp.type === "hello") return;
      if (resp && resp.type === "ok") {
        finish({ ok: true });
      } else if (resp && resp.type === "err") {
        finish({ ok: false, message: String(resp.message || "native host error") });
      } else {
        finish({ ok: false, message: "unexpected native host response" });
      }
    });

    port.onDisconnect.addListener(() => {
      const lastError = chrome.runtime.lastError;
      if (lastError) {
        finish({ ok: false, message: String(lastError.message || "native host disconnected") });
      } else if (!settled) {
        finish({ ok: false, message: "native host disconnected" });
      }
    });

    port.postMessage({
      type: "open_deep_link",
      url,
    });
  });
}

chrome.runtime.onMessage.addListener((msg, _sender, sendResponse) => {
  if (!msg || typeof msg !== "object") {
    return;
  }

  if (msg.type === "COPY_TEXT" && typeof msg.text === "string") {
    (async () => {
      try {
        await navigator.clipboard.writeText(msg.text);
        sendResponse({ ok: true });
      } catch {
        sendResponse({ ok: false });
      }
    })();
    return true;
  }

  if (msg.type === "open_deeplink" && typeof msg.url === "string") {
    (async () => {
      const result = await openDeepLinkViaNative(msg.url);
      sendResponse(result);
    })();
    return true;
  }
});
