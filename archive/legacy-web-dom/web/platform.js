// One bridge for Web, Tauri, and Extension.

async function copyViaNavigator(text) {
  if (typeof navigator !== "undefined" && navigator.clipboard && window.isSecureContext) {
    await navigator.clipboard.writeText(text);
    return true;
  }
  try {
    const ta = document.createElement("textarea");
    ta.value = text;
    ta.style.position = "fixed";
    ta.style.top = "-1000px";
    ta.style.left = "-1000px";
    document.body.appendChild(ta);
    ta.focus();
    ta.select();
    const ok = document.execCommand("copy");
    document.body.removeChild(ta);
    return ok;
  } catch {
    return false;
  }
}

async function copyViaTauri(text) {
  if (globalThis.__TAURI__ && typeof __TAURI__.invoke === "function") {
    try {
      await __TAURI__.invoke("copy_text", { text });
      return true;
    } catch {}
  }
  if (globalThis.__TAURI__?.clipboard?.writeText) {
    try {
      await __TAURI__.clipboard.writeText(text);
      return true;
    } catch {}
  }
  return false;
}

async function copyViaExtension(text) {
  if (typeof chrome !== "undefined" && chrome.runtime?.id) {
    try {
      const ok = await new Promise((res) => {
        chrome.runtime.sendMessage({ type: "COPY_TEXT", text }, (r) => res(!!(r && r.ok)));
      });
      return ok;
    } catch {}
  }
  return false;
}

async function __copy_text(text) {
  if (await copyViaTauri(text)) return true;
  if (await copyViaExtension(text)) return true;
  if (await copyViaNavigator(text)) return true;
  return false;
}

globalThis.__copy_text = __copy_text;
