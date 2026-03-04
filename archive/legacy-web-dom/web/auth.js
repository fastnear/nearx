// Minimal Web/Tauri auth bridge.
// Exposes window.NEARxAuth.{loginGoogle,loginMagic,exchangeCode,setToken,getToken}
;(() => {
  if (window.NEARxAuth) return;
  const AUTH_ORIGIN = window.NEARX_AUTH_ORIGIN || "https://auth.nearx.app";
  const APP_ORIGIN  = location.origin;
  const WEB_CB      = `${APP_ORIGIN}/#/auth/callback`;
  // For Tauri deep-link (if backend supports scheme redirects):
  const TAURI_CB    = `nearx://auth/callback`;

  async function openExternal(url) {
    try {
      // With withGlobalTauri=true, plugins are available under window.__TAURI__
      const t = window.__TAURI__;
      if (t?.opener?.openUrl) {
        await t.opener.openUrl(url);
        return;
      }
      // Robust fallback: direct invoke to plugin command
      if (typeof t?.invoke === 'function') {
        await t.invoke('plugin:opener|open_url', { url });
        return;
      }
    } catch (e) {
      console.warn('[auth] opener plugin failed, falling back:', e);
    }
    window.open(url, '_blank', 'noopener,noreferrer');
  }

  function setToken(t) {
    const token = t || "";
    try { localStorage.setItem("nearx.token", token); } catch {}
    try {
      window.dispatchEvent(new CustomEvent("nearx-auth-token-changed", { detail: { token } }));
    } catch {}
  }
  function getToken() {
    try { return localStorage.getItem("nearx.token") || ""; } catch { return ""; }
  }
  async function sha256(s) {
    const b = new TextEncoder().encode(s);
    const h = await crypto.subtle.digest("SHA-256", b);
    const a = Array.from(new Uint8Array(h));
    return btoa(String.fromCharCode(...a)).replace(/=+$/,"").replace(/\+/g,"-").replace(/\//g,"_");
  }
  function rand(n=64) {
    const bytes = new Uint8Array(n); crypto.getRandomValues(bytes);
    return Array.from(bytes).map(b=>("0"+b.toString(16)).slice(-2)).join("");
  }
  async function loginGoogle() {
    const verifier = rand(64);
    const challenge = await sha256(verifier);
    sessionStorage.setItem("nearx.pkce_verifier", verifier);
    // Prefer Tauri deep-link callback if running in Tauri
    const isTauri = !!(window.__TAURI__ && window.__TAURI__.event);
    const redirect_uri = isTauri ? TAURI_CB : WEB_CB;
    const state = rand(24);
    sessionStorage.setItem("nearx.oauth_state", state);
    const url = `${AUTH_ORIGIN}/v1/oauth/google/start?redirect_uri=${encodeURIComponent(redirect_uri)}&code_challenge=${encodeURIComponent(challenge)}&code_challenge_method=S256&state=${encodeURIComponent(state)}`;
    openExternal(url);
  }
  async function loginMagic() {
    const email = prompt("Enter your email for magic link:");
    if (!email) return;
    const isTauri = !!(window.__TAURI__ && window.__TAURI__.event);
    const redirect_uri = isTauri ? TAURI_CB : WEB_CB;
    try {
      await fetch(`${AUTH_ORIGIN}/v1/magic/start`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ email, redirect_uri })
      });
      alert("Check your email for a login link.");
    } catch (e) {
      console.error("[NEARx][auth] magic start failed", e);
      alert("Unable to send magic link.");
    }
  }
  async function exchangeCode(code) {
    const verifier = sessionStorage.getItem("nearx.pkce_verifier") || "";
    const state = sessionStorage.getItem("nearx.oauth_state") || "";
    if (!code || !verifier) return "";
    try {
      const r = await fetch(`${AUTH_ORIGIN}/v1/oauth/google/exchange`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ code, code_verifier: verifier, state })
      });
      const j = await r.json();
      const token = j && (j.access_token || j.token || "");
      if (token) setToken(token);
      // Immediately scrub token/code from location to prevent accidental leak
      try {
        history.replaceState(null, "", "#/");
      } catch {}
      return token || "";
    } catch (e) {
      console.error("[NEARx][auth] exchange failed", e);
      return "";
    }
  }

  window.NEARxAuth = { loginGoogle, loginMagic, exchangeCode, setToken, getToken };
})();
