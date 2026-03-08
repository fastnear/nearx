import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  isTauriRuntime,
  listNearSigningAccounts,
  listNearSigningKeys,
} from "../tauri/runtime";
import type {
  CredentialSource,
  SigningAccountEntry,
  SigningKeyEntry,
} from "../tauri/runtime";
import {
  keyHasUsableSource,
  preferSigningKey,
  resolveCredentialSource,
  resolveRememberedCredentialSource,
  signingKeyId,
} from "../lib/signerSourceSelection";

interface UseSignerSelectionOptions {
  network: string;
  initialAccountId?: string;
  lastAccountId?: string | null;
  lastPublicKey?: string | null;
  lastCredentialSource?: CredentialSource | null;
  getLastSource: (accountId: string, publicKey: string) => CredentialSource | null;
  sortAccounts: (accounts: SigningAccountEntry[]) => SigningAccountEntry[];
  setLastAccount: (accountId: string) => void;
  setLastKey: (publicKey: string) => void;
  setLastSource: (
    accountId: string,
    publicKey: string,
    source: CredentialSource | null,
  ) => void;
  autoLoad?: boolean;
}

interface SetSelectionParams {
  accountId: string;
  publicKey?: string;
  credentialSource?: CredentialSource | null;
  remember?: boolean;
}

function trimValue(value?: string | null): string {
  return value?.trim() ?? "";
}

export default function useSignerSelection({
  network,
  initialAccountId,
  lastAccountId,
  lastPublicKey,
  lastCredentialSource,
  getLastSource,
  sortAccounts,
  setLastAccount,
  setLastKey,
  setLastSource,
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
  const [selectedCredentialSource, setSelectedCredentialSource] =
    useState<CredentialSource | null>(lastCredentialSource ?? null);
  const loadGenerationRef = useRef(0);
  const loadRef = useRef<((preferredAccountId?: string) => Promise<void>) | null>(null);
  const selectedAccountIdRef = useRef(selectedAccountId);
  const selectedPublicKeyRef = useRef(selectedPublicKey);
  const selectedCredentialSourceRef = useRef(selectedCredentialSource);
  const lastCredentialSourceRef = useRef(lastCredentialSource ?? null);

  useEffect(() => {
    selectedAccountIdRef.current = selectedAccountId;
  }, [selectedAccountId]);

  useEffect(() => {
    selectedPublicKeyRef.current = selectedPublicKey;
  }, [selectedPublicKey]);

  useEffect(() => {
    selectedCredentialSourceRef.current = selectedCredentialSource;
  }, [selectedCredentialSource]);

  useEffect(() => {
    lastCredentialSourceRef.current = lastCredentialSource ?? null;
  }, [lastCredentialSource]);

  const rememberedSourceForKey = useCallback(
    (accountId?: string | null, publicKey?: string | null) => {
      const nextAccountId = trimValue(accountId);
      const nextPublicKey = trimValue(publicKey);
      if (!nextAccountId || !nextPublicKey) {
        return lastCredentialSourceRef.current;
      }
      return getLastSource(nextAccountId, nextPublicKey) ?? lastCredentialSourceRef.current;
    },
    [getLastSource],
  );

  const setCredentialSource = useCallback(
    (
      source: CredentialSource | null,
      remember = true,
      selection?: { accountId?: string | null; publicKey?: string | null },
    ) => {
      selectedCredentialSourceRef.current = source;
      setSelectedCredentialSource(source);
      const accountId = trimValue(selection?.accountId ?? selectedAccountIdRef.current);
      const publicKey = trimValue(selection?.publicKey ?? selectedPublicKeyRef.current);
      if (remember && accountId && publicKey) {
        setLastSource(accountId, publicKey, source);
      }
    },
    [setLastSource],
  );

  const setSelection = useCallback(
    ({ accountId, publicKey, credentialSource, remember = true }: SetSelectionParams) => {
      const nextAccountId = trimValue(accountId);
      const nextPublicKey = trimValue(publicKey);
      selectedAccountIdRef.current = nextAccountId;
      selectedPublicKeyRef.current = nextPublicKey;
      setSelectedAccountId(nextAccountId);
      setSelectedPublicKey(nextPublicKey);
      setCredentialSource(credentialSource ?? null, remember, {
        accountId: nextAccountId,
        publicKey: nextPublicKey,
      });
      if (remember && nextAccountId) {
        setLastAccount(nextAccountId);
      }
      if (remember && nextPublicKey) {
        setLastKey(nextPublicKey);
      }
    },
    [setLastAccount, setLastKey],
  );

  const clearSelection = useCallback(() => {
    selectedAccountIdRef.current = "";
    selectedPublicKeyRef.current = "";
    selectedCredentialSourceRef.current = null;
    setSelectedAccountId("");
    setSelectedPublicKey("");
    setSelectedCredentialSource(null);
  }, []);

  const selectKey = useCallback(
    (entry: SigningKeyEntry, preferredSource?: CredentialSource | null) => {
      setSelection({
        accountId: entry.account_id,
        publicKey: entry.public_key,
        credentialSource: resolveRememberedCredentialSource(
          entry,
          preferredSource ?? rememberedSourceForKey(entry.account_id, entry.public_key),
        ),
      });
    },
    [rememberedSourceForKey, setSelection],
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
          setCredentialSource(
            resolveRememberedCredentialSource(
              current,
              selectedCredentialSourceRef.current ??
                rememberedSourceForKey(current.account_id, current.public_key),
            ),
            true,
            {
              accountId: current.account_id,
              publicKey: current.public_key,
            },
          );
          setLastKey(current.public_key);
          return;
        }

        const fallback = preferSigningKey(keyResult.keys, {
          accountId: targetAccountId,
          publicKey: trimValue(lastPublicKey) || undefined,
        });
        if (fallback) {
          selectKey(
            fallback,
            rememberedSourceForKey(fallback.account_id, fallback.public_key),
          );
          return;
        }

        setSelectedPublicKey("");
        setCredentialSource(null, false);
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
      rememberedSourceForKey,
      network,
      selectKey,
      setLastAccount,
      setLastKey,
      setCredentialSource,
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

  useEffect(() => {
    if (!selectedEntry) {
      return;
    }
    if (
      selectedCredentialSource &&
      selectedEntry.available_sources.includes(selectedCredentialSource)
    ) {
      return;
    }
    setCredentialSource(
      resolveCredentialSource(
        selectedEntry,
        selectedCredentialSourceRef.current ??
          rememberedSourceForKey(selectedEntry.account_id, selectedEntry.public_key),
      ),
      true,
      {
        accountId: selectedEntry.account_id,
        publicKey: selectedEntry.public_key,
      },
    );
  }, [rememberedSourceForKey, selectedCredentialSource, selectedEntry, setCredentialSource]);

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
    selectedCredentialSource,
    selectedEntry,
    selectedKeyId,
    selectedPublicKey,
    setCredentialSource,
    setSelection,
    signingAccounts,
  };
}
