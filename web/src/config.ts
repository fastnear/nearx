const hostname = window.location.hostname;

export const networkId: "mainnet" | "testnet" =
  import.meta.env.VITE_NETWORK_ID === "testnet" ||
  hostname.startsWith("testnet.")
    ? "testnet"
    : "mainnet";

export const apiBaseUrl =
  import.meta.env.VITE_API_BASE_URL ||
  (networkId === "testnet"
    ? "https://tx.test.fastnear.com"
    : "https://tx.main.fastnear.com");

const otherHost =
  networkId === "testnet"
    ? hostname.replace(/^testnet\./, "")
    : `testnet.${hostname}`;

export const otherNetworkUrl = `${window.location.protocol}//${otherHost}`;
export const otherNetworkId = networkId === "testnet" ? "mainnet" : "testnet";
