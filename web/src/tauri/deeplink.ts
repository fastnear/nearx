export function mapCanonicalDeepLinkToRoute(rawUrl: string): string | null {
  let url: URL;

  try {
    url = new URL(rawUrl);
  } catch {
    return null;
  }

  if (url.protocol !== "nearx:" && url.protocol !== "near:") {
    return null;
  }

  const parts = [url.hostname, ...url.pathname.split("/").filter(Boolean)].map(
    decodeURIComponent,
  );

  if (parts.length === 0) {
    return null;
  }

  const base = parts[0] === "v1" ? 1 : 0;
  const route = parts[base];

  if (!route) {
    return null;
  }

  switch (route) {
    case "home": {
      return parts.length === base + 1 ? "/" : null;
    }
    case "tx": {
      const txHash = parts[base + 1];
      return txHash && parts.length === base + 2
        ? `/tx/${encodeURIComponent(txHash)}`
        : null;
    }
    case "block": {
      const blockId = parts[base + 1];
      return blockId && parts.length === base + 2
        ? `/block/${encodeURIComponent(blockId)}`
        : null;
    }
    case "account": {
      const accountId = parts[base + 1];
      return accountId && parts.length === base + 2
        ? `/account/${encodeURIComponent(accountId)}`
        : null;
    }
    default:
      return null;
  }
}
