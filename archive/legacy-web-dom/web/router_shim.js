/**
 * Tiny Hash Router Shim for NEARx Auth Callback
 *
 * Handles "#/auth/callback?..." by:
 * 1. Extracting the query string
 * 2. Calling WASM export nearx_auth_callback(qs)
 * 3. Scrubbing URL to "#/" to prevent token/code leaks
 *
 * Guards against loops using sessionStorage and waits for WASM module
 * to be ready before invoking (up to 2.5 seconds).
 */
(function () {
  const HANDLED_KEY = "nearx.auth.handled"; // Guard against loops

  /**
   * Extract query string from hash (#/auth/callback?foo=bar -> "foo=bar")
   */
  function parseQsFromHash(hash) {
    if (!hash) return "";
    const i = hash.indexOf("?");
    return i >= 0 ? hash.slice(i + 1) : "";
  }

  /**
   * Scrub the URL to #/ and clear the handled flag
   */
  function scrub() {
    try {
      history.replaceState(null, "", "#/");
    } catch (e) {
      console.warn("[NEARx][router] Failed to scrub URL:", e);
    }
    sessionStorage.removeItem(HANDLED_KEY);
  }

  /**
   * Wait for wasm_bindgen exports and invoke nearx_auth_callback
   * @returns {Promise<boolean>} True if successfully invoked
   */
  async function invokeWasm(qs) {
    // Wait for wasm_bindgen exports if needed (up to 2.5 seconds)
    for (let i = 0; i < 50; i++) {
      if (
        window.wasm_bindgen &&
        typeof window.wasm_bindgen.nearx_auth_callback === "function"
      ) {
        try {
          console.log("[NEARx][router] Invoking nearx_auth_callback with qs:", qs);
          window.wasm_bindgen.nearx_auth_callback(qs);
          console.log("[NEARx][router] nearx_auth_callback succeeded");
          return true;
        } catch (e) {
          console.error("[NEARx][router] nearx_auth_callback failed:", e);
          return false;
        }
      }
      await new Promise((r) => setTimeout(r, 50)); // 50ms * 50 = 2.5s max
    }
    console.warn(
      "[NEARx][router] wasm export nearx_auth_callback not ready after 2.5s"
    );
    return false;
  }

  /**
   * Main handler - process auth callback if present
   */
  async function tryHandle() {
    const hash = location.hash || "";

    // Only handle #/auth/callback routes
    if (!hash.startsWith("#/auth/callback")) return;

    // Prevent duplicate handling (e.g., if hash bounces)
    if (sessionStorage.getItem(HANDLED_KEY) === "1") {
      console.log("[NEARx][router] Already handled this callback, skipping");
      return;
    }

    // Extract query string
    const qs = parseQsFromHash(hash);
    if (!qs) {
      console.log("[NEARx][router] No query string in callback URL, scrubbing");
      scrub();
      return;
    }

    // Mark as handled to prevent loops
    sessionStorage.setItem(HANDLED_KEY, "1");
    console.log("[NEARx][router] Handling auth callback with qs:", qs);

    // Invoke WASM export
    const ok = await invokeWasm(qs);

    // Always scrub after handling to avoid token/code lingering in URL
    scrub();

    if (!ok) {
      console.warn(
        "[NEARx][router] WASM invocation may have failed; URL scrubbed anyway"
      );
    } else {
      console.log("[NEARx][router] Auth callback handled successfully");
    }
  }

  // Register handlers
  window.addEventListener("hashchange", tryHandle);
  window.addEventListener("DOMContentLoaded", tryHandle);

  // Also try once on load in case the page opens directly on the callback URL
  tryHandle();

  console.log("[NEARx][router] Router shim loaded and listening");
})();
