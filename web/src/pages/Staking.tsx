import { useState, useEffect, useCallback, useMemo } from "react";
import { Link, useSearchParams } from "react-router-dom";
import { Download, KeyRound, ChevronUp } from "lucide-react";
import {
  listNearCredentials,
  importNearCredentials,
  isTauriRuntime,
} from "../tauri/runtime";
import type { NearCredentialEntry } from "../tauri/runtime";
import { getAccountFull } from "../api/fastnearApi";
import {
  getValidators,
  getPoolBalancesBatch,
  executeStakingAction,
} from "../api/staking";
import type { ValidatorInfo, PoolBalance } from "../api/staking";
import { networkId } from "../config";
import NearAmount from "../components/NearAmount";
import AccountId from "../components/AccountId";
import AccountPicker from "../components/AccountPicker";
import { useAccountPrefs } from "../hooks/useAccountPrefs";

type ActionKind = "deposit_and_stake" | "unstake" | "unstake_all" | "withdraw" | "withdraw_all";

interface ExpandedRow {
  poolId: string;
  action: ActionKind;
}

const NEAR_DECIMALS = 24;

function nearToYocto(near: string): string {
  const parts = near.split(".");
  const whole = parts[0] || "0";
  const frac = (parts[1] || "").padEnd(NEAR_DECIMALS, "0").slice(0, NEAR_DECIMALS);
  const raw = whole + frac;
  return raw.replace(/^0+/, "") || "0";
}

function yoctoToNear(yocto: string): string {
  const padded = yocto.padStart(NEAR_DECIMALS + 1, "0");
  const whole = padded.slice(0, padded.length - NEAR_DECIMALS);
  const frac = padded.slice(padded.length - NEAR_DECIMALS).replace(/0+$/, "");
  return frac ? `${whole}.${frac}` : whole;
}

export default function Staking() {
  const [searchParams] = useSearchParams();
  const initialAccount = searchParams.get("account") || undefined;

  const [accounts, setAccounts] = useState<NearCredentialEntry[]>([]);
  const [accountsLoading, setAccountsLoading] = useState(isTauriRuntime());
  const { lastAccountId, toggleStar, setLastAccount, sortAccounts, isStarred } = useAccountPrefs("staking");

  const [selectedId, setSelectedId] = useState(initialAccount ?? "");
  const [selectedKey, setSelectedKey] = useState("");

  const [poolBalances, setPoolBalances] = useState<PoolBalance[]>([]);
  const [validators, setValidators] = useState<ValidatorInfo[]>([]);
  const [loadProgress, setLoadProgress] = useState<{ done: number; total: number } | null>(null);
  const [dataLoading, setDataLoading] = useState(false);
  const [validatorsLoading, setValidatorsLoading] = useState(false);

  const [expanded, setExpanded] = useState<ExpandedRow | null>(null);
  const [actionAmount, setActionAmount] = useState("");
  const [actionLoading, setActionLoading] = useState(false);
  const [actionResult, setActionResult] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  const [importing, setImporting] = useState(false);
  const [importingAccountId, setImportingAccountId] = useState<string | undefined>();
  const [importMsg, setImportMsg] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const loadAccounts = useCallback(() => {
    if (!isTauriRuntime()) return;
    setAccountsLoading(true);
    console.log("[Staking] listNearCredentials network=%s", networkId);
    listNearCredentials(networkId)
      .then((result) => {
        console.log("[Staking] listNearCredentials result:", JSON.stringify(result, null, 2));
        setAccounts(result.accounts);
        if (result.accounts.length > 0 && !selectedId) {
          const match =
            result.accounts.find((a) => a.account_id === (initialAccount ?? lastAccountId)) ||
            result.accounts[0];
          if (match) {
            setSelectedId(match.account_id);
            setSelectedKey(match.public_key);
          }
        }
      })
      .catch((err) => {
        console.error("[Staking] listNearCredentials error:", err);
        setError(err instanceof Error ? err.message : String(err));
      })
      .finally(() => setAccountsLoading(false));
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    loadAccounts();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const sortedAccounts = sortAccounts(accounts);

  function selectAccount(entry: NearCredentialEntry) {
    setSelectedId(entry.account_id);
    setSelectedKey(entry.public_key);
    setPoolBalances([]);
    setExpanded(null);
    setActionResult(null);
    setActionError(null);
    setLastAccount(entry.account_id);
  }

  // Fetch pool balances when account changes
  useEffect(() => {
    if (!selectedId) return;
    let cancelled = false;
    setDataLoading(true);
    setPoolBalances([]);
    setError(null);

    getAccountFull(selectedId)
      .then((resp) => {
        if (cancelled || !resp) {
          setDataLoading(false);
          return;
        }
        const poolIds = resp.pools.map((p) => p.pool_id);
        if (poolIds.length === 0) {
          setDataLoading(false);
          return;
        }
        setLoadProgress({ done: 0, total: poolIds.length });
        return getPoolBalancesBatch(poolIds, selectedId, (done) => {
          if (!cancelled) setLoadProgress({ done, total: poolIds.length });
        }).then((balances) => {
          if (!cancelled) setPoolBalances(balances);
        });
      })
      .catch((err) => {
        if (!cancelled) setError(err instanceof Error ? err.message : String(err));
      })
      .finally(() => {
        if (!cancelled) {
          setDataLoading(false);
          setLoadProgress(null);
        }
      });

    return () => { cancelled = true; };
  }, [selectedId]);

  // Fetch validators in background
  useEffect(() => {
    let cancelled = false;
    setValidatorsLoading(true);
    getValidators()
      .then((v) => { if (!cancelled) setValidators(v); })
      .catch(() => {})
      .finally(() => { if (!cancelled) setValidatorsLoading(false); });
    return () => { cancelled = true; };
  }, []);

  const highlightedPools = useMemo(
    () => poolBalances.filter((p) => p.staked !== "0" || p.unstaked !== "0"),
    [poolBalances],
  );

  const highlightedPoolIds = useMemo(
    () => new Set(poolBalances.map((p) => p.poolId)),
    [poolBalances],
  );

  const otherValidators = useMemo(
    () => validators.filter((v) => !highlightedPoolIds.has(v.account_id)),
    [validators, highlightedPoolIds],
  );

  const handleImport = useCallback(async (accountId?: string) => {
    setImporting(true);
    setImportingAccountId(accountId);
    setImportMsg(null);
    const params = {
      network: networkId,
      account_id: accountId,
      require_user_presence: true,
      persist_in_keychain: true,
    };
    console.log("[Staking] import_near_credentials request:", JSON.stringify(params));
    try {
      const result = await importNearCredentials(params);
      console.log("[Staking] import_near_credentials result:", JSON.stringify(result, null, 2));
      const count = result.imported_count;
      if (result.failed?.length > 0) {
        console.warn("[Staking] import failures:", JSON.stringify(result.failed, null, 2));
      }
      setImportMsg(
        count > 0
          ? `Imported ${count} credential${count !== 1 ? "s" : ""} to keychain`
          : `No new credentials imported (${result.skipped?.length ?? 0} skipped, ${result.failed?.length ?? 0} failed)`,
      );
      // Optimistically mark imported accounts before full refresh
      if (accountId && count > 0) {
        setAccounts((prev) =>
          prev.map((a) =>
            a.account_id === accountId ? { ...a, in_keychain: true } : a,
          ),
        );
      }
      loadAccounts();
    } catch (err) {
      console.error("[Staking] import_near_credentials error:", err);
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setImporting(false);
      setImportingAccountId(undefined);
    }
  }, [loadAccounts]);

  function toggleExpand(poolId: string, action: ActionKind) {
    if (expanded?.poolId === poolId && expanded.action === action) {
      setExpanded(null);
    } else {
      setExpanded({ poolId, action });
      setActionAmount("");
      setActionResult(null);
      setActionError(null);
    }
  }

  async function handleAction() {
    if (!expanded || !selectedId || !selectedKey) return;
    setActionLoading(true);
    setActionResult(null);
    setActionError(null);

    const needsAmount = expanded.action === "deposit_and_stake" || expanded.action === "unstake" || expanded.action === "withdraw";
    const yoctoAmount = needsAmount ? nearToYocto(actionAmount) : undefined;

    if (needsAmount && (!actionAmount || yoctoAmount === "0")) {
      setActionError("Please enter an amount");
      setActionLoading(false);
      return;
    }

    const labels: Record<ActionKind, string> = {
      deposit_and_stake: `Stake ${actionAmount} NEAR with ${expanded.poolId}`,
      unstake: `Unstake ${actionAmount} NEAR from ${expanded.poolId}`,
      unstake_all: `Unstake all from ${expanded.poolId}`,
      withdraw: `Withdraw ${actionAmount} NEAR from ${expanded.poolId}`,
      withdraw_all: `Withdraw all from ${expanded.poolId}`,
    };

    try {
      const { txHash } = await executeStakingAction({
        action: expanded.action,
        poolId: expanded.poolId,
        signerId: selectedId,
        publicKey: selectedKey,
        amount: yoctoAmount,
        reason: labels[expanded.action],
      });
      setActionResult(txHash);
      // Refresh balances
      setDataLoading(true);
      getAccountFull(selectedId)
        .then((resp) => {
          if (!resp) return;
          const poolIds = resp.pools.map((p) => p.pool_id);
          return getPoolBalancesBatch(poolIds, selectedId);
        })
        .then((balances) => { if (balances) setPoolBalances(balances); })
        .finally(() => setDataLoading(false));
    } catch (err) {
      setActionError(err instanceof Error ? err.message : String(err));
    } finally {
      setActionLoading(false);
    }
  }

  const cardClass = "rounded-lg border border-gray-200 bg-surface text-sm";
  const headerClass = "border-b border-gray-100 px-4 py-2";
  const inputClass = "flex-1 min-w-0 rounded border border-gray-200 bg-gray-50 px-2.5 py-1.5 text-sm font-mono text-gray-900 placeholder:text-gray-400 focus:border-blue-500 focus:outline-none";
  const btnStake = "rounded bg-blue-600 px-3 py-1 text-xs font-medium text-white hover:bg-blue-700 disabled:opacity-50";
  const btnUnstake = "rounded bg-yellow-500 px-3 py-1 text-xs font-medium text-white hover:bg-yellow-600 disabled:opacity-50";
  const btnWithdraw = "rounded bg-green-600 px-3 py-1 text-xs font-medium text-white hover:bg-green-700 disabled:opacity-50";

  return (
    <div>
      <h1 className="mb-4 text-xl font-bold">Staking</h1>

      {/* Account Selector */}
      <div className={`${cardClass} mb-4`}>
        <div className="flex items-center justify-between border-b border-gray-100 px-4 py-2">
          <h2 className="text-xs font-medium uppercase text-gray-500">Account</h2>
          {isTauriRuntime() && accounts.some((a) => a.in_keychain === false) && (
            <button
              onClick={() => handleImport()}
              disabled={importing}
              className="flex items-center gap-1 rounded px-2 py-1 text-xs text-gray-500 hover:bg-gray-100 hover:text-gray-900 disabled:opacity-50"
              title="Import all credentials from ~/.near-credentials to keychain"
            >
              <Download size={12} />
              {importing && !importingAccountId ? "Importing\u2026" : "Import All"}
            </button>
          )}
        </div>
        {accountsLoading ? (
          <div className="px-4 py-3 text-gray-500">Loading accounts&hellip;</div>
        ) : accounts.length === 0 ? (
          <div className="px-4 py-6 text-center">
            <p className="mb-3 text-gray-600">
              No credentials found for <strong>{networkId}</strong>.
            </p>
            <p className="mb-3 text-sm text-gray-500">
              Import from <code className="text-xs bg-gray-100 px-1 py-0.5 rounded">~/.near-credentials/{networkId}/</code> or check the developer console for details.
            </p>
            <button onClick={() => handleImport()} disabled={importing} className={btnStake}>
              <Download size={14} className="mr-1 inline" />
              {importing ? "Importing\u2026" : "Import to Keychain"}
            </button>
          </div>
        ) : (
          <AccountPicker
            accounts={sortedAccounts}
            selectedId={selectedId}
            onSelect={selectAccount}
            onImport={handleImport}
            importing={importing}
            importingAccountId={importingAccountId}
            isStarred={isStarred}
            onToggleStar={toggleStar}
          />
        )}
        {importMsg && (
          <div className="border-t border-gray-100 px-4 py-2 text-xs text-green-600">
            <KeyRound size={12} className="mr-1 inline" />
            {importMsg}
          </div>
        )}
      </div>

      {error && (
        <div className="mb-4 rounded-lg border border-red-200 bg-surface px-4 py-3 text-sm text-red-600">
          {error}
        </div>
      )}

      {/* Loading progress */}
      {dataLoading && loadProgress && (
        <div className="mb-4 text-sm text-gray-500">
          Loading balances&hellip; ({loadProgress.done}/{loadProgress.total} pools)
        </div>
      )}
      {dataLoading && !loadProgress && (
        <div className="mb-4 text-sm text-gray-500">Loading&hellip;</div>
      )}

      {/* Highlighted Validators */}
      {highlightedPools.length > 0 && (
        <div className={`${cardClass} mb-4`}>
          <div className={headerClass}>
            <h2 className="text-xs font-medium uppercase text-gray-500">Your Staked Validators</h2>
          </div>
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-gray-100 text-left text-xs text-gray-500">
                  <th className="px-4 py-2 font-medium">Validator</th>
                  <th className="px-4 py-2 font-medium text-right">Staked</th>
                  <th className="px-4 py-2 font-medium text-right">Unstaked</th>
                  <th className="px-4 py-2 font-medium text-center">Withdraw</th>
                  <th className="px-4 py-2 font-medium text-right">Actions</th>
                </tr>
              </thead>
              <tbody>
                {highlightedPools.map((pool) => (
                  <HighlightedRow
                    key={pool.poolId}
                    pool={pool}
                    expanded={expanded}
                    actionAmount={actionAmount}
                    actionLoading={actionLoading}
                    actionResult={actionResult}
                    actionError={actionError}
                    onToggle={toggleExpand}
                    onAmountChange={setActionAmount}
                    onConfirm={handleAction}
                    btnStake={btnStake}
                    btnUnstake={btnUnstake}
                    btnWithdraw={btnWithdraw}
                    inputClass={inputClass}
                  />
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* No staking positions */}
      {!dataLoading && selectedId && highlightedPools.length === 0 && poolBalances.length >= 0 && (
        <div className={`${cardClass} mb-4`}>
          <div className="px-4 py-4 text-center text-sm text-gray-500">
            No active staking positions. Choose a validator below to stake.
          </div>
        </div>
      )}

      {/* Other Validators */}
      <div className={cardClass}>
        <div className={headerClass}>
          <h2 className="text-xs font-medium uppercase text-gray-500">
            Active Validators
            {validatorsLoading && " (loading\u2026)"}
            {!validatorsLoading && ` (${otherValidators.length})`}
          </h2>
        </div>
        {otherValidators.length > 0 ? (
          <div className="overflow-x-auto max-h-96 overflow-y-auto">
            <table className="w-full text-sm">
              <thead className="sticky top-0 bg-surface">
                <tr className="border-b border-gray-100 text-left text-xs text-gray-500">
                  <th className="px-4 py-2 font-medium">Validator</th>
                  <th className="px-4 py-2 font-medium text-right">Total Stake</th>
                  <th className="px-4 py-2 font-medium text-right">Uptime</th>
                  <th className="px-4 py-2 font-medium text-right">Action</th>
                </tr>
              </thead>
              <tbody>
                {otherValidators.map((v) => {
                  const uptime =
                    v.num_expected_blocks > 0
                      ? ((v.num_produced_blocks / v.num_expected_blocks) * 100).toFixed(1)
                      : "—";
                  const isExpanded = expanded?.poolId === v.account_id && expanded.action === "deposit_and_stake";
                  return (
                    <ValidatorRow
                      key={v.account_id}
                      validator={v}
                      uptime={uptime}
                      isExpanded={isExpanded}
                      actionAmount={actionAmount}
                      actionLoading={actionLoading}
                      actionResult={actionResult}
                      actionError={actionError}
                      onToggle={toggleExpand}
                      onAmountChange={setActionAmount}
                      onConfirm={handleAction}
                      btnStake={btnStake}
                      inputClass={inputClass}
                    />
                  );
                })}
              </tbody>
            </table>
          </div>
        ) : (
          !validatorsLoading && (
            <div className="px-4 py-4 text-center text-sm text-gray-500">
              No validators loaded.
            </div>
          )
        )}
      </div>
    </div>
  );
}

function HighlightedRow({
  pool,
  expanded,
  actionAmount,
  actionLoading,
  actionResult,
  actionError,
  onToggle,
  onAmountChange,
  onConfirm,
  btnStake,
  btnUnstake,
  btnWithdraw,
  inputClass,
}: {
  pool: PoolBalance;
  expanded: ExpandedRow | null;
  actionAmount: string;
  actionLoading: boolean;
  actionResult: string | null;
  actionError: string | null;
  onToggle: (poolId: string, action: ActionKind) => void;
  onAmountChange: (v: string) => void;
  onConfirm: () => void;
  btnStake: string;
  btnUnstake: string;
  btnWithdraw: string;
  inputClass: string;
}) {
  const isExpanded = expanded?.poolId === pool.poolId;
  const activeAction = isExpanded ? expanded!.action : null;

  // Determine max amount based on action type
  const maxYocto = activeAction === "unstake" ? pool.staked
    : activeAction === "withdraw" ? pool.unstaked
    : undefined;

  return (
    <>
      <tr className="border-b border-gray-100">
        <td className="px-4 py-2">
          <AccountId accountId={pool.poolId} maxLength="auto" />
        </td>
        <td className="px-4 py-2 text-right font-mono">
          <NearAmount yoctoNear={pool.staked} />
        </td>
        <td className="px-4 py-2 text-right font-mono">
          <NearAmount yoctoNear={pool.unstaked} />
        </td>
        <td className="px-4 py-2 text-center">
          {pool.canWithdraw === true ? (
            <span className="text-green-600 text-xs">Yes</span>
          ) : pool.canWithdraw === false ? (
            <span className="text-gray-400 text-xs">No</span>
          ) : (
            <span className="text-gray-300 text-xs">—</span>
          )}
        </td>
        <td className="px-4 py-2 text-right">
          <div className="flex items-center justify-end gap-1">
            <button onClick={() => onToggle(pool.poolId, "deposit_and_stake")} className={btnStake}>
              Stake
            </button>
            {pool.staked !== "0" && (
              <button onClick={() => onToggle(pool.poolId, "unstake")} className={btnUnstake}>
                Unstake
              </button>
            )}
            {pool.unstaked !== "0" && pool.canWithdraw && (
              <button onClick={() => onToggle(pool.poolId, "withdraw")} className={btnWithdraw}>
                Withdraw
              </button>
            )}
          </div>
        </td>
      </tr>
      {isExpanded && (
        <tr className="border-b border-gray-100 bg-gray-50">
          <td colSpan={5} className="px-4 py-3">
            <ActionPanel
              action={activeAction!}
              poolId={pool.poolId}
              amount={actionAmount}
              maxYocto={maxYocto}
              loading={actionLoading}
              result={actionResult}
              error={actionError}
              onAmountChange={onAmountChange}
              onConfirm={onConfirm}
              onCancel={() => onToggle(pool.poolId, activeAction!)}
              btnConfirm={activeAction === "unstake" || activeAction === "unstake_all" ? btnUnstake : activeAction === "withdraw" || activeAction === "withdraw_all" ? btnWithdraw : btnStake}
              inputClass={inputClass}
            />
          </td>
        </tr>
      )}
    </>
  );
}

function ValidatorRow({
  validator,
  uptime,
  isExpanded,
  actionAmount,
  actionLoading,
  actionResult,
  actionError,
  onToggle,
  onAmountChange,
  onConfirm,
  btnStake,
  inputClass,
}: {
  validator: ValidatorInfo;
  uptime: string;
  isExpanded: boolean;
  actionAmount: string;
  actionLoading: boolean;
  actionResult: string | null;
  actionError: string | null;
  onToggle: (poolId: string, action: ActionKind) => void;
  onAmountChange: (v: string) => void;
  onConfirm: () => void;
  btnStake: string;
  inputClass: string;
}) {
  return (
    <>
      <tr className="border-b border-gray-100">
        <td className="px-4 py-2">
          <AccountId accountId={validator.account_id} maxLength="auto" />
        </td>
        <td className="px-4 py-2 text-right font-mono">
          <NearAmount yoctoNear={validator.stake} />
        </td>
        <td className="px-4 py-2 text-right">{uptime}%</td>
        <td className="px-4 py-2 text-right">
          <button
            onClick={() => onToggle(validator.account_id, "deposit_and_stake")}
            className={btnStake}
          >
            {isExpanded ? <ChevronUp className="size-3 inline" /> : "Stake"}
          </button>
        </td>
      </tr>
      {isExpanded && (
        <tr className="border-b border-gray-100 bg-gray-50">
          <td colSpan={4} className="px-4 py-3">
            <ActionPanel
              action="deposit_and_stake"
              poolId={validator.account_id}
              amount={actionAmount}
              loading={actionLoading}
              result={actionResult}
              error={actionError}
              onAmountChange={onAmountChange}
              onConfirm={onConfirm}
              onCancel={() => onToggle(validator.account_id, "deposit_and_stake")}
              btnConfirm={btnStake}
              inputClass={inputClass}
            />
          </td>
        </tr>
      )}
    </>
  );
}

function ActionPanel({
  action,
  poolId,
  amount,
  maxYocto,
  loading,
  result,
  error,
  onAmountChange,
  onConfirm,
  onCancel,
  btnConfirm,
  inputClass,
}: {
  action: ActionKind;
  poolId: string;
  amount: string;
  maxYocto?: string;
  loading: boolean;
  result: string | null;
  error: string | null;
  onAmountChange: (v: string) => void;
  onConfirm: () => void;
  onCancel: () => void;
  btnConfirm: string;
  inputClass: string;
}) {
  const needsAmount = action === "deposit_and_stake" || action === "unstake" || action === "withdraw";
  const labels: Record<ActionKind, string> = {
    deposit_and_stake: "Stake",
    unstake: "Unstake",
    unstake_all: "Unstake All",
    withdraw: "Withdraw",
    withdraw_all: "Withdraw All",
  };

  const maxNear = maxYocto && maxYocto !== "0" ? yoctoToNear(maxYocto) : undefined;

  return (
    <div className="space-y-2">
      <div className="flex items-center gap-2 text-xs text-gray-500">
        <span className="font-medium">{labels[action]}</span>
        <span>&rarr;</span>
        <span className="font-mono">{poolId}</span>
      </div>
      {needsAmount && (
        <div className="flex items-center gap-2">
          <input
            className={`${inputClass} max-w-48`}
            value={amount}
            onChange={(e) => onAmountChange(e.target.value)}
            placeholder="0.0"
            type="text"
            disabled={loading}
          />
          <span className="text-xs text-gray-500">NEAR</span>
          {maxNear && (
            <button
              type="button"
              onClick={() => onAmountChange(maxNear)}
              disabled={loading}
              className="rounded border border-gray-300 px-2 py-1 text-xs font-medium text-gray-600 hover:bg-gray-100 disabled:opacity-50"
            >
              Max
            </button>
          )}
        </div>
      )}
      {maxNear && needsAmount && (
        <div className="text-xs text-gray-400">
          Available: {maxNear} NEAR
        </div>
      )}
      <div className="flex items-center gap-2">
        <button onClick={onConfirm} disabled={loading} className={btnConfirm}>
          {loading ? "Processing\u2026" : `Confirm ${labels[action]}`}
        </button>
        <button onClick={onCancel} disabled={loading} className="text-xs text-gray-500 hover:text-gray-700">
          Cancel
        </button>
      </div>
      {result && (
        <div className="text-xs text-green-600">
          Success! TX:{" "}
          <Link to={`/tx/${result}`} className="text-blue-600 hover:underline font-mono">
            {result.slice(0, 12)}&hellip;
          </Link>
        </div>
      )}
      {error && <div className="text-xs text-red-600">{error}</div>}
    </div>
  );
}
