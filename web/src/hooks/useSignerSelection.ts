import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  isTauriRuntime,
  listNearSigningAccounts,
  listNearSigningKeys,
} from "../tauri/runtime";
import type {
  SigningAccountEntry,
  SigningKeyEntry,
} from "../tauri/runtime";
import {
  keyHasUsableSource,
  preferSigningKey,
  signingKeyId,
} from "../lib/signerSourceSelection";

interface UseSignerSelectionOptions {
  network: string;
  initialAccountId?: string;
  lastAccountId?: string | null;
  lastPublicKey?: string | null;
  sortAccounts: (accounts: SigningAccountEntry[]) => SigningAccountEntry[];
  setLastAccount: (accountId: string) => void;
  setLastKey: (publicKey: string) => void;
  autoLoad?: boolean;
}

function trimValue(value?: string | null): string {
  return value?.trim() ?? "";
}

export default function useSignerSelection({
  network,
  initialAccountId,
  lastAccountId,
  lastPublicKey,
  sortAccounts,
  setLastAccount,
  setLastKey,
  autoLoad = true,
}: UseSignerSelectionOptions) {
  const [signingAccounts, setSigningAccounts] = useState<SigningAccountEntry[]>([]);
  const [keys, setKeys] = useState<SigningKeyEntry[]>([]);
  const [accountsLoading, setAccountsLoading] = useState(isTauriRuntime());
  const [loadError, setLoadError] = useState<string | null>(null);
  const [selectedAccountId, setSelectedAccountId] = useState(
    trimValue(initialAccountId) || trimValue(lastAccountId),
  );
  const [selectedPublicKey, setSelectedPublicKey] = useState("");
  const loadGenerationRef = useRef(0);
  const loadRef = useRef<((preferredAccountId?: string) => Promise<void>) | null>(null);
  const selectedAccountIdRef = useRef(selectedAccountId);
  const selectedPublicKeyRef = useRef(selectedPublicKey);

  useEffect(() => {
    selectedAccountIdRef.current = selectedAccountId;
  }, [selectedAccountId]);

  useEffect(() => {
    selectedPublicKeyRef.current = selectedPublicKey;
  }, [selectedPublicKey]);

  const setSelection = useCallback(
    ({ accountId, publicKey }: { accountId: string; publicKey?: string }) => {
      const nextAccountId = trimValue(accountId);
      const nextPublicKey = trimValue(publicKey);
      selectedAccountIdRef.current = nextAccountId;
      selectedPublicKeyRef.current = nextPublicKey;
      setSelectedAccountId(nextAccountId);
      setSelectedPublicKey(nextPublicKey);
      if (nextAccountId) {
        setLastAccount(nextAccountId);
      }
      if (nextPublicKey) {
        setLastKey(nextPublicKey);
      }
    },
    [setLastAccount, setLastKey],
  );

  const clearSelection = useCallback(() => {
    selectedAccountIdRef.current = "";
    selectedPublicKeyRef.current = "";
    setSelectedAccountId("");
    setSelectedPublicKey("");
  }, []);

  const selectKey = useCallback(
    (entry: SigningKeyEntry) => {
      setSelection({
        accountId: entry.account_id,
        publicKey: entry.public_key,
      });
    },
    [setSelection],
  );

  const load = useCallback(
    async (preferredAccountId?: string) => {
      const generation = ++loadGenerationRef.current;
      if (!isTauriRuntime()) {
        return;
      }

      setAccountsLoading(true);
      setLoadError(null);

      try {
        const accountResult = await listNearSigningAccounts({ network });
        if (generation !== loadGenerationRef.current) {
          return;
        }

        const accounts = sortAccounts(accountResult.accounts);
        setSigningAccounts(accounts);

        const targetAccountId =
          trimValue(preferredAccountId) ||
          trimValue(selectedAccountIdRef.current) ||
          trimValue(initialAccountId) ||
          trimValue(lastAccountId) ||
          accounts.find((entry) => entry.has_keys)?.account_id ||
          accounts[0]?.account_id ||
          "";

        if (!targetAccountId) {
          setKeys([]);
          clearSelection();
          return;
        }

        setSelectedAccountId(targetAccountId);
        if (targetAccountId) {
          setLastAccount(targetAccountId);
        }

        const keyResult = await listNearSigningKeys({
          network,
          account_id: targetAccountId,
        });
        if (generation !== loadGenerationRef.current) {
          return;
        }

        setKeys(keyResult.keys);

        const current = keyResult.keys.find(
          (key) =>
            key.account_id === targetAccountId &&
            key.public_key === trimValue(selectedPublicKeyRef.current),
        );
        if (current) {
          setSelectedPublicKey(current.public_key);
          setLastKey(current.public_key);
          return;
        }

        const fallback = preferSigningKey(keyResult.keys, {
          accountId: targetAccountId,
          publicKey: trimValue(lastPublicKey) || undefined,
        });
        if (fallback) {
          selectKey(fallback);
          return;
        }

        setSelectedPublicKey("");
      } catch (err) {
        if (generation === loadGenerationRef.current) {
          setLoadError(err instanceof Error ? err.message : String(err));
        }
      } finally {
        if (generation === loadGenerationRef.current) {
          setAccountsLoading(false);
        }
      }
    },
    [
      clearSelection,
      initialAccountId,
      lastAccountId,
      lastPublicKey,
      network,
      selectKey,
      setLastAccount,
      setLastKey,
      sortAccounts,
    ],
  );

  const selectAccount = useCallback(
    async (accountId: string) => {
      const nextAccountId = trimValue(accountId);
      if (!nextAccountId) {
        clearSelection();
        setKeys([]);
        return;
      }
      selectedAccountIdRef.current = nextAccountId;
      setSelectedAccountId(nextAccountId);
      setKeys([]);
      setSelectedPublicKey("");
      selectedPublicKeyRef.current = "";
      await load(nextAccountId);
    },
    [clearSelection, load],
  );

  useEffect(() => {
    loadRef.current = load;
  }, [load]);

  useEffect(() => {
    if (!autoLoad) {
      return;
    }
    void loadRef.current?.(trimValue(initialAccountId) || undefined);
  }, [autoLoad, initialAccountId]);

  const selectedEntry = useMemo(
    () =>
      keys.find(
        (key) =>
          key.account_id === trimValue(selectedAccountId) &&
          key.public_key === trimValue(selectedPublicKey),
      ) ?? null,
    [keys, selectedAccountId, selectedPublicKey],
  );

  const allSignableKeys = useMemo(
    () => keys.filter((key) => keyHasUsableSource(key)),
    [keys],
  );

  const selectedKeyId = useMemo(
    () => (selectedEntry ? signingKeyId(selectedEntry) : ""),
    [selectedEntry],
  );

  return {
    accountsLoading,
    allSignableKeys,
    clearSelection,
    keys,
    load,
    loadError,
    selectAccount,
    selectKey,
    selectedAccountId,
    selectedEntry,
    selectedKeyId,
    selectedPublicKey,
    setSelection,
    signingAccounts,
  };
}
