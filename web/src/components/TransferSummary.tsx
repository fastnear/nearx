import { useState } from "react";
import type { TransferInfo, NftTransferInfo } from "../utils/parseTransaction";
import useTokenMetadata, { type TokenMetadata } from "../hooks/useTokenMetadata";
import useMultiTokenMetadata, {
  type MultiTokenMetadata,
} from "../hooks/useMultiTokenMetadata";
import useTokenPrices, {
  getFtPrice,
  getMtPrice,
  computeUsdValue,
  formatUsd,
} from "../hooks/useTokenPrices";
import useSpamTokens, { isSpam } from "../hooks/useSpamTokens";
import AccountId from "./AccountId";
import NearAmount from "./NearAmount";
import { CircleAlert, CircleStop, Coins, Image, Loader2 } from "lucide-react";
import useSpamNfts, { isSpamNft } from "../hooks/useSpamNfts";

function formatTokenAmount(amount: string, decimals: number): string {
  if (amount === "0") return "0";
  const padded = amount.padStart(decimals + 1, "0");
  const whole =
    padded.slice(0, padded.length - decimals).replace(/^0+/, "") || "0";
  const frac = padded.slice(padded.length - decimals).replace(/0+$/, "");
  if (!frac) return whole;
  return `${whole}.${frac.slice(0, 5)}`;
}

function TokenIcon({ src, alt }: { src: string; alt: string }) {
  return (
    <img
      src={src}
      alt={alt}
      className="inline-block size-3.5 rounded-full object-cover align-text-bottom"
    />
  );
}

const MAX_SYMBOL_LEN = 15;

function truncateSymbol(symbol: string): string {
  return symbol.length > MAX_SYMBOL_LEN
    ? symbol.slice(0, MAX_SYMBOL_LEN) + "\u2026"
    : symbol;
}

export function TokenAmount({
  amount,
  meta,
  contractId,
  tokenId,
}: {
  amount: string;
  meta: TokenMetadata;
  contractId?: string;
  tokenId?: string;
}) {
  const [showRaw, setShowRaw] = useState(false);
  const { prices } = useTokenPrices();
  const spamSet = useSpamTokens();
  const spam = isSpam(spamSet, contractId ?? "");
  const priceInfo = getFtPrice(prices, contractId);
  const usdValue = priceInfo
    ? computeUsdValue(amount, priceInfo.decimals, priceInfo.price)
    : 0;
  const symbol = truncateSymbol(meta.symbol);
  return (
    <span
      className={`cursor-pointer font-mono hover:underline decoration-dotted inline-flex items-center gap-0.5 ${showRaw ? "flex-wrap break-all" : "whitespace-nowrap"}`}
      onClick={() => setShowRaw(!showRaw)}
      title={
        showRaw
          ? `Click to show in ${meta.symbol}`
          : "Click to show raw units"
      }
    >
      {showRaw
        ? `${amount} units ${symbol}`
        : `${formatTokenAmount(amount, meta.decimals)} ${symbol}`}
      {meta.icon && <TokenIcon src={meta.icon} alt={meta.symbol} />}
      {spam && <span title="Flagged as spam"><CircleAlert className="inline-block size-3 shrink-0 text-red-500" /></span>}
      {!showRaw && usdValue > 0 && (
        <span className="text-gray-400 font-sans text-xs">{formatUsd(usdValue)}</span>
      )}
      {showRaw && contractId && (
        <>
          <AccountId accountId={contractId} />
          {tokenId && (
            <span className="text-gray-400 break-all" title={tokenId}>
              :{tokenId.length > 150 ? tokenId.slice(0, 147) + "\u2026" : tokenId}
            </span>
          )}
        </>
      )}
    </span>
  );
}

function MultiTokenAmount({
  amount,
  meta,
  contractId,
  tokenId,
}: {
  amount: string;
  meta: MultiTokenMetadata;
  contractId?: string;
  tokenId?: string;
}) {
  const [showRaw, setShowRaw] = useState(false);
  const { prices } = useTokenPrices();
  const spamSet = useSpamTokens();
  const spam = isSpam(spamSet, contractId ?? "");
  const priceInfo = getMtPrice(prices, contractId, tokenId);
  const usdValue = priceInfo
    ? computeUsdValue(amount, priceInfo.decimals, priceInfo.price)
    : 0;
  const label = truncateSymbol(meta.symbol ?? meta.title);
  const hasDecimals = meta.decimals != null && meta.decimals > 0;
  const clickable = hasDecimals || contractId;
  return (
    <span
      className={`font-mono inline-flex items-center gap-0.5${clickable ? " cursor-pointer hover:underline decoration-dotted" : ""} ${showRaw ? "flex-wrap break-all" : "whitespace-nowrap"}`}
      onClick={clickable ? () => setShowRaw(!showRaw) : undefined}
      title={
        clickable
          ? showRaw
            ? `Click to show in ${meta.symbol ?? meta.title}`
            : "Click to show raw units"
          : (meta.description ?? meta.title)
      }
    >
      {hasDecimals && !showRaw
        ? `${formatTokenAmount(amount, meta.decimals!)} ${label}`
        : `${amount} units ${label}`}
      {meta.media && <TokenIcon src={meta.media} alt={meta.title} />}
      {spam && <span title="Flagged as spam"><CircleAlert className="inline-block size-3 shrink-0 text-red-500" /></span>}
      {!showRaw && usdValue > 0 && (
        <span className="text-gray-400 font-sans text-xs">{formatUsd(usdValue)}</span>
      )}
      {showRaw && contractId && (
        <>
          <AccountId accountId={contractId} />
          {tokenId && (
            <span className="text-gray-400 break-all" title={tokenId}>
              :{tokenId.length > 150 ? tokenId.slice(0, 147) + "\u2026" : tokenId}
            </span>
          )}
        </>
      )}
    </span>
  );
}

function MintBadge({ tokenIcon }: { tokenIcon?: React.ReactNode }) {
  return (
    <span className="inline-flex items-center gap-0.5 whitespace-nowrap font-mono text-xs text-green-500">
      {tokenIcon}
      <span>Mint</span>
    </span>
  );
}

function BurnBadge({ tokenIcon }: { tokenIcon?: React.ReactNode }) {
  return (
    <span className="inline-flex items-center gap-0.5 whitespace-nowrap font-mono text-xs text-orange-400">
      {tokenIcon}
      <span>Burn</span>
    </span>
  );
}

function RawTokenAmount({
  amount,
  contractId,
  tokenId,
}: {
  amount: string;
  contractId?: string;
  tokenId?: string;
}) {
  return (
    <span className="font-mono whitespace-nowrap">
      {amount}{" "}
      {contractId && <AccountId accountId={contractId} />}
      {tokenId && (
        <span className="text-gray-400" title={tokenId}>
          :{tokenId.length > 20 ? tokenId.slice(0, 17) + "\u2026" : tokenId}
        </span>
      )}
    </span>
  );
}

export default function TransferSummary({
  transfer,
}: {
  transfer: TransferInfo;
}) {
  // Determine token ID prefix for nep245 tokens
  const hasNep141Prefix = transfer.tokenId?.startsWith("nep141:");
  const hasNep245Prefix = transfer.tokenId?.startsWith("nep245:");
  const isNativeMt =
    transfer.tokenType === "nep245" && !hasNep141Prefix && !hasNep245Prefix;
  // Use MT metadata for native MT tokens and nep245:-prefixed tokens
  const usesMtLookup = isNativeMt || !!hasNep245Prefix;

  // FT metadata — for nep141 tokens and nep245 with nep141: prefix
  const ftContractId = !usesMtLookup
    ? transfer.tokenType === "near"
      ? null
      : transfer.tokenType === "nep141"
        ? (transfer.contractId ?? null)
        : hasNep141Prefix && transfer.tokenId
          ? transfer.tokenId.slice(7)
          : null
    : null;
  const { metadata: ftMeta, loading: ftLoading } =
    useTokenMetadata(ftContractId);

  // For nep245: prefix, extract inner contract + token ID
  // e.g. "nep245:contract.near:42" → contract "contract.near", token "42"
  const nep245Inner = hasNep245Prefix && transfer.tokenId
    ? (() => {
        const rest = transfer.tokenId!.slice(7);
        const colonIdx = rest.indexOf(":");
        return colonIdx >= 0
          ? { contract: rest.slice(0, colonIdx), token: rest.slice(colonIdx + 1) }
          : { contract: rest, token: null };
      })()
    : null;

  // MT metadata — for native nep245 tokens and nep245: prefix
  const mtContract = isNativeMt
    ? (transfer.contractId ?? null)
    : nep245Inner?.contract ?? null;
  const mtTokenId = isNativeMt
    ? (transfer.tokenId ?? null)
    : nep245Inner?.token ?? null;
  const { metadata: mtMeta, loading: mtLoading } = useMultiTokenMetadata(
    mtContract,
    mtTokenId,
  );

  const loading = usesMtLookup ? mtLoading : ftLoading;
  const tokenIcon =
    transfer.tokenType === "nep245"
      ? <Coins className="size-3" />
      : transfer.tokenType === "nep141"
        ? <CircleStop className="size-3" />
        : <span className="text-xs leading-none">Ⓝ</span>;

  return (
    <span className="inline-flex flex-wrap items-center gap-1">
      {transfer.from !== null && transfer.to !== null && (
        <span className="inline-flex items-center gap-0.5 whitespace-nowrap font-mono text-xs text-gray-400">
          {tokenIcon}
        </span>
      )}
      {transfer.from === null && <MintBadge tokenIcon={tokenIcon} />}
      {transfer.to === null && <BurnBadge tokenIcon={tokenIcon} />}
      {transfer.from !== null && <AccountId accountId={transfer.from} />}
      <span className="text-gray-400">&rarr;</span>
      {transfer.to !== null && <AccountId accountId={transfer.to} />}
      {transfer.tokenType === "near" ? (
        <NearAmount yoctoNear={transfer.amount} showPrice />
      ) : loading ? (
        <Loader2 className="size-3 animate-spin text-gray-400" />
      ) : usesMtLookup && mtMeta ? (
        <MultiTokenAmount
          amount={transfer.amount}
          meta={mtMeta}
          contractId={transfer.contractId}
          tokenId={transfer.tokenId}
        />
      ) : !usesMtLookup && ftMeta ? (
        <TokenAmount
          amount={transfer.amount}
          meta={ftMeta}
          contractId={transfer.contractId}
          tokenId={transfer.tokenId}
        />
      ) : (
        <RawTokenAmount
          amount={transfer.amount}
          contractId={transfer.contractId}
          tokenId={
            transfer.tokenType === "nep245" ? transfer.tokenId : undefined
          }
        />
      )}
    </span>
  );
}

const MAX_TOKEN_ID_LEN = 20;

function truncateTokenId(tokenId: string): string {
  return tokenId.length > MAX_TOKEN_ID_LEN
    ? tokenId.slice(0, MAX_TOKEN_ID_LEN - 3) + "\u2026"
    : tokenId;
}

export function NftTransferSummary({
  transfer,
}: {
  transfer: NftTransferInfo;
}) {
  const spamData = useSpamNfts();
  const spam = isSpamNft(spamData, transfer.contractId, transfer.tokenId);
  const isMint = transfer.from === null;
  const isBurn = transfer.to === null;
  const nftLabel = (
    <>
      <Image className="size-3" />
      <span>NFT</span>
    </>
  );

  return (
    <span className="inline-flex flex-wrap items-center gap-1">
      {!isMint && !isBurn && (
        <span className="inline-flex items-center gap-0.5 whitespace-nowrap font-mono text-xs text-gray-400">
          {nftLabel}
        </span>
      )}
      {isMint && <MintBadge tokenIcon={nftLabel} />}
      {isBurn && <BurnBadge tokenIcon={nftLabel} />}
      {transfer.from !== null && <AccountId accountId={transfer.from} />}
      <span className="text-gray-400">&rarr;</span>
      {transfer.to !== null && <AccountId accountId={transfer.to} />}
      <span className="font-mono whitespace-nowrap text-gray-500" title={transfer.tokenId}>
        #{truncateTokenId(transfer.tokenId)}
      </span>
      {spam && (
        <span title="Flagged as spam">
          <CircleAlert className="inline-block size-3 shrink-0 text-red-500" />
        </span>
      )}
    </span>
  );
}
