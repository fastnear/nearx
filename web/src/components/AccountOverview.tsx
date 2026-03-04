import { useState, useMemo } from "react";
import type {
  AccountFullResponse,
  AccountFtToken,
  AccountNft,
} from "../api/fastnearApi";
import useTokenMetadata from "../hooks/useTokenMetadata";
import useTokenPrices, {
  computeUsdValue,
  type TokenPrice,
} from "../hooks/useTokenPrices";
import useSpamTokens, { isSpam } from "../hooks/useSpamTokens";
import useSpamNfts, { isSpamNft } from "../hooks/useSpamNfts";
import type { AccountState } from "../api/rpc";
import { TokenAmount } from "./TransferSummary";
import NearAmount from "./NearAmount";
import AccountId from "./AccountId";
import { ChevronDown, ChevronRight, Code, Coins, Image, Landmark, Loader2 } from "lucide-react";

const VISIBLE_LIMIT = 3;

function formatStorageBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function isNonZeroBalance(token: AccountFtToken): boolean {
  return token.balance !== "" && token.balance !== "0";
}

function getTokenUsdValue(
  token: AccountFtToken,
  prices: Map<string, TokenPrice> | null
): number {
  if (!prices) return 0;
  const p = prices.get(token.contract_id);
  if (!p) return 0;
  return computeUsdValue(token.balance, p.decimals, p.price);
}

function FtTokenRow({ token }: { token: AccountFtToken }) {
  const { metadata, loading } = useTokenMetadata(token.contract_id);
  const spamSet = useSpamTokens();
  const spam = isSpam(spamSet, token.contract_id);

  const spamClass = spam ? "opacity-50" : "";

  if (loading) {
    return (
      <div className={`flex items-center gap-2 py-1 text-sm ${spamClass}`}>
        <Loader2 className="size-3 animate-spin text-gray-400" />
        <AccountId accountId={token.contract_id} />
      </div>
    );
  }

  if (!metadata) {
    return (
      <div className={`flex items-center gap-2 py-1 text-sm ${spamClass}`}>
        <span className="font-mono">{token.balance}</span>
        <AccountId accountId={token.contract_id} />
      </div>
    );
  }

  return (
    <div className={`flex items-center gap-2 py-1 text-sm ${spamClass}`}>
      <TokenAmount
        amount={token.balance}
        meta={metadata}
        contractId={token.contract_id}
      />
    </div>
  );
}

function CollapsibleSection({
  icon,
  title,
  count,
  defaultOpen = true,
  hidden = false,
  children,
}: {
  icon: React.ReactNode;
  title: string;
  count: number;
  defaultOpen?: boolean;
  hidden?: boolean;
  children: React.ReactNode;
}) {
  const [open, setOpen] = useState(defaultOpen);

  if (hidden) return null;

  return (
    <div>
      <button
        className="flex w-full items-center gap-1.5 text-left text-sm font-medium text-gray-700 hover:text-gray-900"
        onClick={() => setOpen(!open)}
      >
        {open ? <ChevronDown className="size-4" /> : <ChevronRight className="size-4" />}
        {icon}
        {title}
        <span className="font-normal text-gray-400">({count})</span>
      </button>
      {open && <div className="mt-2">{children}</div>}
    </div>
  );
}

function ExpandableList<T>({
  items,
  renderItem,
}: {
  items: T[];
  renderItem: (item: T, index: number) => React.ReactNode;
}) {
  const [expanded, setExpanded] = useState(false);
  const visible = expanded ? items : items.slice(0, VISIBLE_LIMIT);
  const hiddenCount = items.length - VISIBLE_LIMIT;

  return (
    <>
      <div>
        {visible.map((item, i) => renderItem(item, i))}
      </div>
      {hiddenCount > 0 && !expanded && (
        <button
          className="mt-1 text-sm text-blue-600 hover:underline"
          onClick={() => setExpanded(true)}
        >
          + {hiddenCount} more
        </button>
      )}
    </>
  );
}

function useSortedTokens(
  data: AccountFullResponse | null,
  prices: Map<string, TokenPrice> | null,
  spamSet: Set<string> | null
): AccountFtToken[] {
  return useMemo(() => {
    if (!data) return [];
    const nonZero = data.tokens.filter(isNonZeroBalance);
    return [...nonZero].sort((a, b) => {
      // Spam tokens always go to the bottom
      const aSpam = isSpam(spamSet, a.contract_id);
      const bSpam = isSpam(spamSet, b.contract_id);
      if (aSpam !== bSpam) return aSpam ? 1 : -1;
      // Then sort by USD value descending
      if (!prices) return 0;
      return getTokenUsdValue(b, prices) - getTokenUsdValue(a, prices);
    });
  }, [data, prices, spamSet]);
}

function TokenList({
  tokens,
  spamTokens,
  zeroTokens,
}: {
  tokens: AccountFtToken[];
  spamTokens: AccountFtToken[];
  zeroTokens: AccountFtToken[];
}) {
  const [expanded, setExpanded] = useState(false);
  const [showSpam, setShowSpam] = useState(false);
  const [showZero, setShowZero] = useState(false);
  const visible = expanded ? tokens : tokens.slice(0, VISIBLE_LIMIT);
  const hiddenCount = tokens.length - VISIBLE_LIMIT;

  return (
    <>
      <div>
        {visible.map((token) => (
          <FtTokenRow key={token.contract_id} token={token} />
        ))}
      </div>
      {hiddenCount > 0 && !expanded && (
        <button
          className="mt-1 text-sm text-blue-600 hover:underline"
          onClick={() => setExpanded(true)}
        >
          + {hiddenCount} more
        </button>
      )}
      {(expanded || tokens.length <= VISIBLE_LIMIT) && spamTokens.length > 0 && !showSpam && (
        <button
          className="mt-1 block text-sm text-gray-400 hover:text-gray-600"
          onClick={() => setShowSpam(true)}
        >
          + {spamTokens.length} spam token{spamTokens.length !== 1 ? "s" : ""}
        </button>
      )}
      {showSpam && (
        <div>
          {spamTokens.map((token) => (
            <FtTokenRow key={token.contract_id} token={token} />
          ))}
        </div>
      )}
      {(expanded || tokens.length <= VISIBLE_LIMIT) && zeroTokens.length > 0 && !showZero && (
        <button
          className="mt-1 block text-sm text-gray-400 hover:text-gray-600"
          onClick={() => setShowZero(true)}
        >
          + {zeroTokens.length} zero-balance token{zeroTokens.length !== 1 ? "s" : ""}
        </button>
      )}
      {showZero && (
        <div>
          {zeroTokens.map((token) => (
            <FtTokenRow key={token.contract_id} token={token} />
          ))}
        </div>
      )}
    </>
  );
}

function NftList({
  nfts,
  spamNfts,
}: {
  nfts: AccountNft[];
  spamNfts: AccountNft[];
}) {
  const [expanded, setExpanded] = useState(false);
  const [showSpam, setShowSpam] = useState(false);
  const visible = expanded ? nfts : nfts.slice(0, VISIBLE_LIMIT);
  const hiddenCount = nfts.length - VISIBLE_LIMIT;

  return (
    <>
      <div>
        {visible.map((nft) => (
          <div key={nft.contract_id} className="py-0.5">
            <AccountId accountId={nft.contract_id} />
          </div>
        ))}
      </div>
      {hiddenCount > 0 && !expanded && (
        <button
          className="mt-1 text-sm text-blue-600 hover:underline"
          onClick={() => setExpanded(true)}
        >
          + {hiddenCount} more
        </button>
      )}
      {(expanded || nfts.length <= VISIBLE_LIMIT) && spamNfts.length > 0 && !showSpam && (
        <button
          className="mt-1 block text-sm text-gray-400 hover:text-gray-600"
          onClick={() => setShowSpam(true)}
        >
          + {spamNfts.length} spam NFT contract{spamNfts.length !== 1 ? "s" : ""}
        </button>
      )}
      {showSpam && (
        <div>
          {spamNfts.map((nft) => (
            <div key={nft.contract_id} className="py-0.5 opacity-50">
              <AccountId accountId={nft.contract_id} />
            </div>
          ))}
        </div>
      )}
    </>
  );
}

export default function AccountOverview({
  data,
  loading,
  error,
  accountState,
  stateLoading,
}: {
  data: AccountFullResponse | null;
  loading: boolean;
  error: string | null;
  accountState?: AccountState | null;
  stateLoading?: boolean;
}) {
  const { prices } = useTokenPrices();
  const spamSet = useSpamTokens();
  const spamNfts = useSpamNfts();
  const sortedTokens = useSortedTokens(data, prices, spamSet);
  const { nftsClean, nftsSpam } = useMemo(() => {
    const nfts = data?.nfts ?? [];
    if (!spamNfts) return { nftsClean: nfts, nftsSpam: [] as typeof nfts };
    const clean: typeof nfts = [];
    const spam: typeof nfts = [];
    for (const nft of nfts) {
      if (isSpamNft(spamNfts, nft.contract_id)) spam.push(nft);
      else clean.push(nft);
    }
    return { nftsClean: clean, nftsSpam: spam };
  }, [data, spamNfts]);
  const { tokensClean, tokensSpam } = useMemo(() => {
    if (!spamSet) return { tokensClean: sortedTokens, tokensSpam: [] as AccountFtToken[] };
    const clean: AccountFtToken[] = [];
    const spam: AccountFtToken[] = [];
    for (const t of sortedTokens) {
      if (isSpam(spamSet, t.contract_id)) spam.push(t);
      else clean.push(t);
    }
    return { tokensClean: clean, tokensSpam: spam };
  }, [sortedTokens, spamSet]);

  if (loading) {
    return (
      <div className="mb-6 flex items-center gap-2 text-sm text-gray-500">
        <Loader2 className="size-4 animate-spin" />
        Loading account overview...
      </div>
    );
  }

  if (error) {
    return (
      <p className="mb-6 text-sm text-red-600">
        Failed to load account overview
      </p>
    );
  }

  if (!data) return null;

  const { state, tokens, pools } = data;

  // RPC is not loaded yet and fastnear says account doesn't exist
  if (!accountState && !stateLoading && !state) {
    return (
      <p className="mb-6 text-sm text-gray-500">
        This account does not exist on-chain.
      </p>
    );
  }

  // RPC loaded but account not found
  if (accountState === null && !stateLoading && !state) {
    return (
      <p className="mb-6 text-sm text-gray-500">
        This account does not exist on-chain.
      </p>
    );
  }

  // Prefer RPC data; fall back to fastnear API data
  const balance = accountState?.amount ?? state?.balance;
  const locked = accountState?.locked ?? state?.locked;
  const storageBytes = accountState?.storage_usage ?? state?.storage_bytes;

  const zeroTokens = tokens.filter((t) => !isNonZeroBalance(t));

  return (
    <div className="mb-6 space-y-3 rounded-lg border border-gray-200 bg-surface px-4 py-4 text-sm">
      {/* State summary */}
      <dl className="grid gap-px sm:grid-cols-2 [&>div]:flex [&>div]:min-w-0 [&>div]:gap-2 [&>div]:py-1">
        {balance && (
          <div>
            <dt className="shrink-0 text-gray-500">Balance</dt>
            <dd><NearAmount yoctoNear={balance} showPrice /></dd>
          </div>
        )}
        {locked && locked !== "0" && (
          <div>
            <dt className="shrink-0 text-gray-500">Locked (Staked)</dt>
            <dd><NearAmount yoctoNear={locked} showPrice /></dd>
          </div>
        )}
        {storageBytes != null && (
          <div>
            <dt className="shrink-0 text-gray-500">Storage Used</dt>
            <dd>{formatStorageBytes(storageBytes)}</dd>
          </div>
        )}
        {stateLoading && !accountState && (
          <div>
            <dt className="shrink-0 text-gray-500">Contract</dt>
            <dd><Loader2 className="size-3 animate-spin text-gray-400" /></dd>
          </div>
        )}
        {accountState && (
          <div>
            <dt className="shrink-0 text-gray-500">Contract</dt>
            <dd className="flex items-center gap-1.5">
              {accountState.hasContract ? (
                <>
                  <Code className="size-3.5 text-green-600" />
                  <span className="text-green-700">Deployed</span>
                  <span className="font-mono text-xs text-gray-400" title={accountState.code_hash}>
                    {accountState.code_hash.slice(0, 8)}&hellip;
                  </span>
                </>
              ) : (
                <span className="text-gray-400">None</span>
              )}
            </dd>
          </div>
        )}
      </dl>

      <div className="sm:columns-2 sm:gap-0">
        <div className="break-inside-avoid border-t border-gray-100 px-1 py-3">
          <CollapsibleSection
            icon={<Coins className="size-4" />}
            title="Tokens"
            count={tokensClean.length + tokensSpam.length + zeroTokens.length}
            hidden={tokensClean.length === 0 && tokensSpam.length === 0 && zeroTokens.length === 0}
          >
            <TokenList
              tokens={tokensClean}
              spamTokens={tokensSpam}
              zeroTokens={zeroTokens}
            />
          </CollapsibleSection>
        </div>

        <div className="break-inside-avoid border-t border-gray-100 px-1 py-3">
          <CollapsibleSection
            icon={<Image className="size-4" />}
            title="NFT Contracts"
            count={nftsClean.length + nftsSpam.length}
            hidden={nftsClean.length === 0 && nftsSpam.length === 0}
          >
            <NftList nfts={nftsClean} spamNfts={nftsSpam} />
          </CollapsibleSection>
        </div>

        <div className="break-inside-avoid border-t border-gray-100 px-1 py-3">
          <CollapsibleSection
            icon={<Landmark className="size-4" />}
            title="Staking Pools"
            count={pools.length}
            defaultOpen={pools.length > 0}
          >
            <ExpandableList
              items={pools}
              renderItem={(pool) => (
                <div key={pool.pool_id} className="py-0.5">
                  <AccountId accountId={pool.pool_id} />
                </div>
              )}
            />
          </CollapsibleSection>
        </div>
      </div>
    </div>
  );
}
