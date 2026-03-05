import { useState, useEffect, useCallback } from "react";
import { Download, KeyRound } from "lucide-react";
import { viewAccessKey, broadcastTransaction } from "../api/rpc";
import {
  signTransaction,
  listNearCredentials,
  importNearCredentials,
  requestUserPresence,
  isTauriRuntime,
} from "../tauri/runtime";
import type {
  SignTransactionResult,
  NearCredentialEntry,
} from "../tauri/runtime";
import { networkId } from "../config";
import { useAccountPrefs } from "../hooks/useAccountPrefs";
import AccountPicker from "../components/AccountPicker";

type ActionType = "Transfer" | "FunctionCall";

const rowClass =
  "flex items-baseline gap-3 border-b border-gray-100 px-4 py-2.5 last:border-b-0";
const labelClass = "shrink-0 w-24 text-right text-gray-500";
const inputClass =
  "flex-1 min-w-0 rounded border border-gray-200 bg-gray-50 px-2.5 py-1.5 text-sm font-mono text-gray-900 placeholder:text-gray-400 focus:border-blue-500 focus:outline-none";

export default function SignTransaction() {
  const [accounts, setAccounts] = useState<NearCredentialEntry[]>([]);
  const [accountsLoading, setAccountsLoading] = useState(false);
  const { lastAccountId, toggleStar, setLastAccount, sortAccounts, isStarred } = useAccountPrefs("sign");
  const [manualMode, setManualMode] = useState(false);

  const [signerId, setSignerId] = useState("");
  const [publicKey, setPublicKey] = useState("");
  const [receiverId, setReceiverId] = useState("");
  const [actionType, setActionType] = useState<ActionType>("Transfer");

  const [deposit, setDeposit] = useState("1");

  const [methodName, setMethodName] = useState("");
  const [argsJson, setArgsJson] = useState("{}");
  const [gas, setGas] = useState("30000000000000");
  const [fnDeposit, setFnDeposit] = useState("0");

  const [signResult, setSignResult] = useState<SignTransactionResult | null>(null);
  const [broadcastResult, setBroadcastResult] = useState<unknown>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [importing, setImporting] = useState(false);
  const [importingAccountId, setImportingAccountId] = useState<string | undefined>();
  const [importMsg, setImportMsg] = useState<string | null>(null);
  const [debugLog, setDebugLog] = useState<string[]>([]);

  const loadAccounts = useCallback(() => {
    if (!isTauriRuntime()) {
      console.log("[SignTransaction] not Tauri runtime, manual mode");
      setManualMode(true);
      return;
    }
    setAccountsLoading(true);
    console.log("[SignTransaction] listNearCredentials network=%s", networkId);
    listNearCredentials(networkId)
      .then((result) => {
        console.log("[SignTransaction] listNearCredentials result:", JSON.stringify(result, null, 2));
        if (result.accounts.length === 0) {
          setManualMode(true);
        } else {
          setAccounts(result.accounts);
          setManualMode(false);
        }
      })
      .catch((err) => {
        console.error("[SignTransaction] listNearCredentials error:", err);
        setManualMode(true);
      })
      .finally(() => setAccountsLoading(false));
  }, []);

  useEffect(() => {
    loadAccounts();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const sortedAccounts = sortAccounts(accounts);

  useEffect(() => {
    if (manualMode || accounts.length === 0 || signerId) return;
    const match =
      accounts.find((a) => a.account_id === lastAccountId) ||
      sortedAccounts[0];
    if (match) selectAccount(match);
  }, [accounts, manualMode]); // eslint-disable-line react-hooks/exhaustive-deps

  function selectAccount(entry: NearCredentialEntry) {
    setSignerId(entry.account_id);
    setPublicKey(entry.public_key);
    setReceiverId(entry.account_id);
    setActionType("Transfer");
    setDeposit("1");
    setSignResult(null);
    setBroadcastResult(null);
    setError(null);
    setLastAccount(entry.account_id);
  }

  const addDebug = useCallback((msg: string) => {
    const ts = new Date().toLocaleTimeString();
    const line = `[${ts}] ${msg}`;
    console.log("[SignTransaction]", msg);
    setDebugLog((prev) => [...prev, line]);
  }, []);

  const handleImport = useCallback(async (accountId?: string) => {
    setImporting(true);
    setImportingAccountId(accountId);
    setImportMsg(null);
    setError(null);
    const params = {
      network: networkId,
      account_id: accountId,
      require_user_presence: true,
      persist_in_keychain: true,
    };
    addDebug(`import_near_credentials request: ${JSON.stringify(params)}`);
    try {
      const result = await importNearCredentials(params);
      addDebug(`import_near_credentials response: ${JSON.stringify(result, null, 2)}`);
      const count = result.imported_count;
      if (result.failed && result.failed.length > 0) {
        addDebug(`import failures: ${JSON.stringify(result.failed, null, 2)}`);
      }
      if (result.skipped && result.skipped.length > 0) {
        addDebug(`import skipped: ${JSON.stringify(result.skipped, null, 2)}`);
      }
      setImportMsg(
        count > 0
          ? `Imported ${count} credential${count !== 1 ? "s" : ""} to keychain`
          : `No new credentials imported (${result.skipped?.length ?? 0} skipped, ${result.failed?.length ?? 0} failed)`,
      );
      if (accountId && count > 0) {
        setAccounts((prev) =>
          prev.map((a) =>
            a.account_id === accountId ? { ...a, in_keychain: true } : a,
          ),
        );
      }
      loadAccounts();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      addDebug(`import_near_credentials ERROR: ${msg}`);
      setError(msg);
    } finally {
      setImporting(false);
      setImportingAccountId(undefined);
    }
  }, [loadAccounts, addDebug]);

  const handleSign = useCallback(async () => {
    setError(null);
    setSignResult(null);
    setBroadcastResult(null);
    setLoading(true);
    try {
      // Check if any deposit is attached
      const effectiveDeposit = actionType === "Transfer" ? deposit : fnDeposit;
      const hasDeposit = !!effectiveDeposit && effectiveDeposit !== "0";

      if (hasDeposit && isTauriRuntime()) {
        const depositDesc = actionType === "Transfer"
          ? `Transfer ${deposit} yoctoNEAR to ${receiverId}`
          : `Call ${methodName} on ${receiverId} with ${fnDeposit} yoctoNEAR deposit`;
        addDebug(`Deposit attached — requesting user presence: ${depositDesc}`);
        const presence = await requestUserPresence(depositDesc);
        addDebug(`user presence result: ${JSON.stringify(presence)}`);
        if (!presence.verified) {
          setError("User presence verification failed — signing aborted");
          setLoading(false);
          return;
        }
      }

      addDebug(`viewAccessKey(${signerId}, ${publicKey})`);
      const ak = await viewAccessKey(signerId, publicKey);
      addDebug(`access key: nonce=${ak.nonce}, block_hash=${ak.block_hash}`);

      let actions;
      if (actionType === "Transfer") {
        actions = [{ type: "Transfer" as const, deposit }];
      } else {
        const argsBase64 = btoa(argsJson);
        actions = [
          {
            type: "FunctionCall" as const,
            method_name: methodName,
            args: argsBase64,
            gas: Number(gas),
            deposit: fnDeposit,
          },
        ];
      }

      const signParams = {
        signer_id: signerId,
        receiver_id: receiverId,
        nonce: ak.nonce + 1,
        block_hash: ak.block_hash,
        actions,
        network: networkId,
        reason: `Sign ${actionType} to ${receiverId}`,
      };
      addDebug(`sign_transaction request: ${JSON.stringify(signParams, null, 2)}`);

      const result = await signTransaction(signParams);
      addDebug(`sign_transaction success: tx_hash=${result.tx_hash}`);
      setSignResult(result);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      addDebug(`sign_transaction ERROR: ${msg}`);
      setError(msg);
    } finally {
      setLoading(false);
    }
  }, [signerId, publicKey, receiverId, actionType, deposit, methodName, argsJson, gas, fnDeposit, addDebug]);

  const handleBroadcast = useCallback(async () => {
    if (!signResult) return;
    setError(null);
    setBroadcastResult(null);
    setLoading(true);
    try {
      const result = await broadcastTransaction(signResult.signed_transaction_base64);
      setBroadcastResult(result);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, [signResult]);

  return (
    <div>
      <h1 className="mb-4 text-xl font-bold">Sign Transaction</h1>

      {/* ── Signer card ── */}
      <div className="mb-4 rounded-lg border border-gray-200 bg-surface text-sm">
        <div className="flex items-center justify-between border-b border-gray-100 px-4 py-2">
          <h2 className="text-xs font-medium uppercase text-gray-500">Signer</h2>
          <div className="flex items-center gap-1.5">
            {!manualMode && accounts.some((a) => a.in_keychain === false) && isTauriRuntime() && (
              <button
                type="button"
                onClick={() => handleImport()}
                disabled={importing}
                className="flex items-center gap-1 rounded px-2 py-0.5 text-xs text-gray-500 hover:bg-gray-100 hover:text-gray-900 disabled:opacity-50"
                title="Import all credentials from ~/.near-credentials to keychain"
              >
                <Download size={11} />
                {importing && !importingAccountId ? "Importing\u2026" : "Import All"}
              </button>
            )}
            {!manualMode && sortedAccounts.length > 0 && (
              <button
                type="button"
                onClick={() => setManualMode(true)}
                className="rounded px-2 py-0.5 text-xs text-gray-500 hover:bg-gray-100 hover:text-gray-900"
              >
                Manual
              </button>
            )}
            {manualMode && accounts.length > 0 && (
              <button
                type="button"
                onClick={() => {
                  setManualMode(false);
                  const match =
                    accounts.find((a) => a.account_id === lastAccountId) ||
                    sortedAccounts[0];
                  if (match) selectAccount(match);
                }}
                className="rounded px-2 py-0.5 text-xs text-blue-600 hover:bg-gray-100"
              >
                Use List
              </button>
            )}
          </div>
        </div>

        {accountsLoading ? (
          <div className="px-4 py-3 text-gray-500">Loading accounts\u2026</div>
        ) : !manualMode && sortedAccounts.length > 0 ? (
          <>
            <AccountPicker
              accounts={sortedAccounts}
              selectedId={signerId}
              onSelect={selectAccount}
              onImport={handleImport}
              importing={importing}
              importingAccountId={importingAccountId}
              isStarred={isStarred}
              onToggleStar={toggleStar}
            />
            {publicKey && (
              <div className={`border-t border-gray-100 ${rowClass}`}>
                <span className={labelClass}>Public Key</span>
                <code className="flex-1 min-w-0 break-all text-xs text-gray-500">{publicKey}</code>
              </div>
            )}
          </>
        ) : (
          <>
            <div className={rowClass}>
              <span className={labelClass}>Account</span>
              <input
                className={inputClass}
                value={signerId}
                onChange={(e) => setSignerId(e.target.value)}
                placeholder="alice.near"
              />
            </div>
            <div className={rowClass}>
              <span className={labelClass}>Public Key</span>
              <input
                className={inputClass}
                value={publicKey}
                onChange={(e) => setPublicKey(e.target.value)}
                placeholder="ed25519:..."
              />
            </div>
          </>
        )}

        {importMsg && (
          <div className="border-t border-gray-100 px-4 py-2 text-xs text-green-600">
            <KeyRound size={12} className="mr-1 inline" />
            {importMsg}
          </div>
        )}
      </div>

      {/* ── Transaction card ── */}
      <div className="mb-4 rounded-lg border border-gray-200 bg-surface text-sm">
        <div className="border-b border-gray-100 px-4 py-2">
          <h2 className="text-xs font-medium uppercase text-gray-500">Transaction</h2>
        </div>

        <div className={rowClass}>
          <span className={labelClass}>Receiver</span>
          <input
            className={inputClass}
            value={receiverId}
            onChange={(e) => setReceiverId(e.target.value)}
            placeholder="bob.near"
          />
        </div>

        <div className={rowClass}>
          <span className={labelClass}>Action</span>
          <select
            className={inputClass}
            value={actionType}
            onChange={(e) => setActionType(e.target.value as ActionType)}
          >
            <option value="Transfer">Transfer</option>
            <option value="FunctionCall">Function Call</option>
          </select>
        </div>

        {actionType === "Transfer" && (
          <div className={rowClass}>
            <span className={labelClass}>Deposit</span>
            <input
              className={inputClass}
              value={deposit}
              onChange={(e) => setDeposit(e.target.value)}
              placeholder="1"
            />
            <span className="shrink-0 text-xs text-gray-400">yocto</span>
          </div>
        )}

        {actionType === "FunctionCall" && (
          <>
            <div className={rowClass}>
              <span className={labelClass}>Method</span>
              <input
                className={inputClass}
                value={methodName}
                onChange={(e) => setMethodName(e.target.value)}
                placeholder="ft_transfer"
              />
            </div>
            <div className="border-b border-gray-100 px-4 py-2.5">
              <div className="flex items-baseline gap-3">
                <span className={labelClass}>Args (JSON)</span>
                <textarea
                  className={inputClass + " h-20 font-mono"}
                  value={argsJson}
                  onChange={(e) => setArgsJson(e.target.value)}
                  placeholder='{"receiver_id": "bob.near", "amount": "1000"}'
                />
              </div>
            </div>
            <div className={rowClass}>
              <span className={labelClass}>Gas</span>
              <input
                className={inputClass}
                value={gas}
                onChange={(e) => setGas(e.target.value)}
                placeholder="30000000000000"
              />
            </div>
            <div className={rowClass}>
              <span className={labelClass}>Deposit</span>
              <input
                className={inputClass}
                value={fnDeposit}
                onChange={(e) => setFnDeposit(e.target.value)}
                placeholder="0"
              />
              <span className="shrink-0 text-xs text-gray-400">yocto</span>
            </div>
          </>
        )}

        <div className="flex items-center gap-3 px-4 py-3">
          <button
            onClick={handleSign}
            disabled={loading || !signerId || !publicKey || !receiverId}
            className="rounded bg-blue-600 px-5 py-1.5 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {loading && !signResult ? "Signing\u2026" : "Sign"}
          </button>
          {signResult && (
            <button
              onClick={handleBroadcast}
              disabled={loading}
              className="rounded bg-green-600 px-5 py-1.5 text-sm font-medium text-white hover:bg-green-700 disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {loading && signResult ? "Broadcasting\u2026" : "Broadcast"}
            </button>
          )}
        </div>
      </div>

      {/* ── Error ── */}
      {error && (
        <div className="mb-4 rounded-lg border border-red-200 bg-surface px-4 py-3 text-sm text-red-600">
          {error}
        </div>
      )}

      {/* ── Signed result ── */}
      {signResult && (
        <div className="mb-4 rounded-lg border border-gray-200 bg-surface text-sm">
          <div className="border-b border-gray-100 px-4 py-2">
            <h2 className="text-xs font-medium uppercase text-gray-500">Signed Transaction</h2>
          </div>
          <div className={rowClass}>
            <span className={labelClass}>TX Hash</span>
            <code className="flex-1 min-w-0 break-all text-xs">{signResult.tx_hash}</code>
          </div>
          <div className={rowClass}>
            <span className={labelClass}>Signer</span>
            <code className="flex-1 min-w-0 break-all text-xs">{signResult.signer_id}</code>
          </div>
          <div className={rowClass}>
            <span className={labelClass}>Public Key</span>
            <code className="flex-1 min-w-0 break-all text-xs">{signResult.public_key}</code>
          </div>
          <div className="border-b border-gray-100 px-4 py-2.5 last:border-b-0">
            <details>
              <summary className="flex items-baseline gap-3 cursor-pointer">
                <span className={labelClass}>Payload</span>
                <span className="text-xs text-gray-500 hover:text-gray-900">
                  Base64 ({signResult.signed_transaction_base64.length} chars)
                </span>
              </summary>
              <pre className="mt-2 overflow-x-auto rounded bg-gray-100 p-2.5 text-xs break-all whitespace-pre-wrap">
                {signResult.signed_transaction_base64}
              </pre>
            </details>
          </div>
        </div>
      )}

      {/* ── Broadcast result ── */}
      {broadcastResult !== null && (
        <div className="mb-4 rounded-lg border border-gray-200 bg-surface text-sm">
          <div className="border-b border-gray-100 px-4 py-2">
            <h2 className="text-xs font-medium uppercase text-green-600">Broadcast Result</h2>
          </div>
          <div className="px-4 py-3">
            <pre className="overflow-x-auto rounded bg-gray-100 p-2.5 text-xs">
              {JSON.stringify(broadcastResult, null, 2)}
            </pre>
          </div>
        </div>
      )}

      {/* ── Debug log ── */}
      {debugLog.length > 0 && (
        <div className="mb-4 rounded-lg border border-gray-200 bg-surface text-sm">
          <div className="flex items-center justify-between border-b border-gray-100 px-4 py-2">
            <h2 className="text-xs font-medium uppercase text-gray-500">Debug Log</h2>
            <button
              type="button"
              onClick={() => setDebugLog([])}
              className="text-xs text-gray-400 hover:text-gray-600"
            >
              Clear
            </button>
          </div>
          <pre className="max-h-64 overflow-auto px-4 py-2.5 text-xs font-mono text-gray-600 whitespace-pre-wrap">
            {debugLog.join("\n")}
          </pre>
        </div>
      )}
    </div>
  );
}
