export type NetworkId = "mainnet" | "testnet";

const TESTNET_PREFIX = "testnet.";

const CANONICAL_BASE_URL: Record<NetworkId, string> = {
  mainnet: "https://near.rocks",
  testnet: "https://testnet.near.rocks",
};

const IPV4_RE = /^(?:\d{1,3}\.){3}\d{1,3}$/;

export function inferAccountNetwork(accountId: string): NetworkId | null {
  const value = accountId.trim().toLowerCase();
  if (!value) return null;
  if (value.endsWith(".testnet")) return "testnet";
  if (value.endsWith(".near") || value.endsWith(".tg")) return "mainnet";
  return null;
}

export function isDerivableSiblingHost(hostname: string): boolean {
  const host = hostname.trim().toLowerCase();
  if (!host) return false;
  if (host === "localhost" || host.endsWith(".localhost")) return false;
  if (host.startsWith("[") && host.endsWith("]")) return false;
  if (host.includes(":")) return false;
  if (IPV4_RE.test(host)) return false;

  const labels = host.split(".").filter(Boolean);
  if (labels.length < 2) return false;

  const tld = labels[labels.length - 1];
  if (!/^[a-z]{2,63}$/.test(tld)) return false;

  return labels.every(
    (label) =>
      /^[a-z0-9-]+$/.test(label) &&
      !label.startsWith("-") &&
      !label.endsWith("-"),
  );
}

type CurrentLocation = Pick<
  Location,
  "protocol" | "hostname" | "port" | "pathname" | "search" | "hash"
>;

type BuildCrossNetworkUrlParams = {
  currentLocation: CurrentLocation;
  targetNetwork: NetworkId;
  routePath?: string;
  search?: string;
  hash?: string;
};

type BuildCrossNetworkAccountUrlParams = {
  currentLocation: CurrentLocation;
  targetNetwork: NetworkId;
  accountId: string;
};

function toSiblingHost(hostname: string, targetNetwork: NetworkId): string {
  const host = hostname.toLowerCase();
  if (targetNetwork === "testnet") {
    return host.startsWith(TESTNET_PREFIX) ? host : `${TESTNET_PREFIX}${host}`;
  }

  return host.startsWith(TESTNET_PREFIX)
    ? host.slice(TESTNET_PREFIX.length)
    : host;
}

function normalizePath(pathname: string): string {
  if (!pathname) return "/";
  return pathname.startsWith("/") ? pathname : `/${pathname}`;
}

function normalizeSearch(search: string): string {
  if (!search) return "";
  return search.startsWith("?") ? search : `?${search}`;
}

function normalizeHash(hash: string): string {
  if (!hash) return "";
  return hash.startsWith("#") ? hash : `#${hash}`;
}

export function buildCrossNetworkUrl(params: BuildCrossNetworkUrlParams): string {
  const { currentLocation, targetNetwork } = params;
  const protocol =
    currentLocation.protocol === "http:" || currentLocation.protocol === "https:"
      ? currentLocation.protocol
      : "https:";

  const path = normalizePath(params.routePath ?? currentLocation.pathname);
  const search = normalizeSearch(params.search ?? currentLocation.search);
  const hash = normalizeHash(params.hash ?? currentLocation.hash);

  let base: string;
  if (isDerivableSiblingHost(currentLocation.hostname)) {
    const siblingHost = toSiblingHost(currentLocation.hostname, targetNetwork);
    const port = currentLocation.port ? `:${currentLocation.port}` : "";
    base = `${protocol}//${siblingHost}${port}`;
  } else {
    base = CANONICAL_BASE_URL[targetNetwork];
  }

  return `${base}${path}${search}${hash}`;
}

export function buildCrossNetworkAccountUrl(
  params: BuildCrossNetworkAccountUrlParams,
): string {
  const accountId = params.accountId.trim();
  return buildCrossNetworkUrl({
    currentLocation: params.currentLocation,
    targetNetwork: params.targetNetwork,
    routePath: `/account/${encodeURIComponent(accountId)}`,
    search: "",
    hash: "",
  });
}
