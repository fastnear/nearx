/**
 * Tauri Deep Link Bridge
 *
 * Listens for deep link events from Tauri and updates the browser location hash
 * so the WASM app can parse and handle them.
 *
 * This file is safe to include in plain web builds - it will no-op if Tauri APIs
 * are not available.
 */

(function() {
    'use strict';

    // Check if we're running in Tauri
    if (!window.__TAURI__ || !window.__TAURI__.event) {
        console.log('[deep_link] Not running in Tauri, deep link bridge disabled');
        return;
    }

    console.log('[deep_link] Tauri detected, initializing deep link bridge');

    // Listen for deep link events from Tauri
    window.__TAURI__.event.listen('nearx://open', function(event) {
        try {
            const url = event && event.payload ? String(event.payload) : '';

            if (!url) {
                console.warn('[deep_link] Received empty URL');
                return;
            }

            console.log('[deep_link] Received deep link:', url);

            // Encode the URL and set it as a hash
            // Format: #/deeplink/<encodeURIComponent(nearx://...)>
            const encoded = encodeURIComponent(url);
            const newHash = '#/deeplink/' + encoded;

            // Update location hash - this will trigger the WASM app to parse it
            window.location.hash = newHash;

            console.log('[deep_link] Updated location hash:', newHash);
        } catch (err) {
            console.error('[deep_link] Error processing deep link:', err);
        }
    });

    console.log('[deep_link] Bridge initialized, listening for nearx://open events');
})();
