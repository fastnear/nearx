// Cloudflare Snippet for *.near.rocks / *.testnet.near.rocks / *.tg.near.rocks
// Proxied DNS wildcard records point to a non-existing IP; this snippet intercepts
// and redirects to the appropriate explorer page.
//
// Redirect rules:
//   alex.near.rocks              → near.rocks/account/alex.near
//   <64-hex>.near.rocks          → near.rocks/account/<64-hex>  (implicit account)
//   alice.testnet.near.rocks     → testnet.near.rocks/account/alice.testnet
//   <64-hex>.testnet.near.rocks  → testnet.near.rocks/account/<64-hex>  (implicit account)
//   123512343.testnet.near.rocks → testnet.near.rocks/block/123512343
//   <base58-txhash>.testnet.near.rocks → testnet.near.rocks/tx/<base58-txhash>
//   bob.tg.near.rocks            → near.rocks/account/bob.tg
//   123512343.near.rocks         → near.rocks/block/123512343
//   <base58-txhash>.near.rocks   → near.rocks/tx/<base58-txhash>

const MAINNET_BASE = "https://near.rocks";
const TESTNET_BASE = "https://testnet.near.rocks";

const BASE58_ALPHABET =
  "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
const BASE58_SET = new Set(BASE58_ALPHABET);

function decodeBase58(str) {
  const bytes = [];
  for (const c of str) {
    if (!BASE58_SET.has(c)) return null;
    let carry = BASE58_ALPHABET.indexOf(c);
    for (let j = 0; j < bytes.length; j++) {
      carry += bytes[j] * 58;
      bytes[j] = carry & 0xff;
      carry >>= 8;
    }
    while (carry > 0) {
      bytes.push(carry & 0xff);
      carry >>= 8;
    }
  }
  for (const c of str) {
    if (c !== "1") break;
    bytes.push(0);
  }
  return bytes.length;
}

// 64-hex implicit account (ed25519 public key)
const HEX64_RE = /^[0-9a-f]{64}$/;

function detectType(subdomain) {
  // Block number: all digits
  if (/^\d+$/.test(subdomain)) return "block";
  // Implicit account: 64 hex chars (must check before tx, since some hex is valid base58)
  if (HEX64_RE.test(subdomain)) return "implicit";
  // TX hash: base58-encoded 32 bytes, typically 43-44 chars
  if (subdomain.length < 50 && decodeBase58(subdomain) === 32) return "tx";
  // Everything else is a named account
  return "account";
}

export default {
  async fetch(request) {
    const url = new URL(request.url);
    const host = url.hostname.toLowerCase();

    // *.testnet.near.rocks → block, tx, implicit, or {sub}.testnet account
    if (host.endsWith(".testnet.near.rocks")) {
      const sub = host.slice(0, -".testnet.near.rocks".length);
      if (sub) {
        const type = detectType(sub);
        if (type === "block") {
          return Response.redirect(`${TESTNET_BASE}/block/${sub}`, 302);
        }
        if (type === "tx") {
          return Response.redirect(`${TESTNET_BASE}/tx/${sub}`, 302);
        }
        if (type === "implicit") {
          return Response.redirect(`${TESTNET_BASE}/account/${sub}`, 302);
        }
        return Response.redirect(`${TESTNET_BASE}/account/${sub}.testnet`, 302);
      }
    }

    // *.tg.near.rocks → {sub}.tg account
    if (host.endsWith(".tg.near.rocks")) {
      const sub = host.slice(0, -".tg.near.rocks".length);
      if (sub) {
        return Response.redirect(`${MAINNET_BASE}/account/${sub}.tg`, 302);
      }
    }

    // *.near.rocks → block, tx, implicit, or {sub}.near account
    if (host.endsWith(".near.rocks")) {
      const sub = host.slice(0, -".near.rocks".length);
      if (sub) {
        const type = detectType(sub);
        if (type === "block") {
          return Response.redirect(`${MAINNET_BASE}/block/${sub}`, 302);
        }
        if (type === "tx") {
          return Response.redirect(`${MAINNET_BASE}/tx/${sub}`, 302);
        }
        if (type === "implicit") {
          return Response.redirect(`${MAINNET_BASE}/account/${sub}`, 302);
        }
        return Response.redirect(`${MAINNET_BASE}/account/${sub}.near`, 302);
      }
    }

    // Bare near.rocks or unmatched — pass through
    return fetch(request);
  },
};
