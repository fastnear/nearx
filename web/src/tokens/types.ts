export interface FtMetadata {
  name: string;
  symbol: string;
  decimals: number;
  icon?: string;
  contractId: string;
  fetchedAt: number; // 0 = bundled/never expires
}
