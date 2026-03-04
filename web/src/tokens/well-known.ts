import type { FtMetadata } from "./types";

const mainnetTokens: FtMetadata[] = [
  {
    contractId: "wrap.near",
    name: "Wrapped NEAR",
    symbol: "wNEAR",
    decimals: 24,
    fetchedAt: 0,
  },
  {
    contractId: "usdt.tether-token.near",
    name: "Tether USD",
    symbol: "USDt",
    decimals: 6,
    fetchedAt: 0,
  },
  {
    contractId: "17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1",
    name: "USDC",
    symbol: "USDC",
    decimals: 6,
    fetchedAt: 0,
  },
  {
    contractId: "token.sweat",
    name: "Sweat Economy",
    symbol: "SWEAT",
    decimals: 18,
    fetchedAt: 0,
  },
  {
    contractId: "aaaaaa20d9e0e2461697782ef11675f668207961.factory.bridge.near",
    name: "Aurora",
    symbol: "AURORA",
    decimals: 18,
    fetchedAt: 0,
  },
  {
    contractId: "token.v2.ref-finance.near",
    name: "Ref Finance",
    symbol: "REF",
    decimals: 18,
    fetchedAt: 0,
  },
  {
    contractId: "linear-protocol.near",
    name: "LiNEAR",
    symbol: "LiNEAR",
    decimals: 24,
    fetchedAt: 0,
  },
  {
    contractId: "meta-pool.near",
    name: "Staked NEAR",
    symbol: "stNEAR",
    decimals: 24,
    fetchedAt: 0,
  },
  {
    contractId: "dac17f958d2ee523a2206206994597c13d831ec7.factory.bridge.near",
    name: "Tether USD (Bridged)",
    symbol: "USDT.e",
    decimals: 6,
    fetchedAt: 0,
  },
  {
    contractId: "a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48.factory.bridge.near",
    name: "USD Coin (Bridged)",
    symbol: "USDC.e",
    decimals: 6,
    fetchedAt: 0,
  },
];

const testnetTokens: FtMetadata[] = [
  {
    contractId: "wrap.testnet",
    name: "Wrapped NEAR",
    symbol: "wNEAR",
    decimals: 24,
    fetchedAt: 0,
  },
];

const wellKnownMap: Record<string, Map<string, FtMetadata>> = {
  mainnet: new Map(mainnetTokens.map((t) => [t.contractId, t])),
  testnet: new Map(testnetTokens.map((t) => [t.contractId, t])),
};

export function getWellKnownToken(
  contractId: string,
  network: string
): FtMetadata | undefined {
  return wellKnownMap[network]?.get(contractId);
}
