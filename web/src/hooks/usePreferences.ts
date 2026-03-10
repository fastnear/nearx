import { useState, useEffect, useCallback } from "react";
import {
  getPreferences,
  setPreferences,
  isTauriRuntime,
} from "../tauri/runtime";
import type { UserPreferences } from "../tauri/runtime";

const defaultPreferences: UserPreferences = {
  always_prompt_user_presence: false,
};

export function usePreferences() {
  const [preferences, setPreferencesState] =
    useState<UserPreferences>(defaultPreferences);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!isTauriRuntime()) {
      setLoading(false);
      return;
    }

    let cancelled = false;
    void getPreferences()
      .then((result) => {
        if (!cancelled) {
          setPreferencesState(result.preferences);
        }
      })
      .catch((err) => {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : String(err));
        }
      })
      .finally(() => {
        if (!cancelled) {
          setLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, []);

  const updatePreference = useCallback(
    async <K extends keyof UserPreferences>(key: K, value: UserPreferences[K]) => {
      const next = { ...preferences, [key]: value };
      setPreferencesState(next);
      setError(null);
      try {
        const result = await setPreferences(next);
        setPreferencesState(result.preferences);
      } catch (err) {
        setPreferencesState(preferences);
        setError(err instanceof Error ? err.message : String(err));
      }
    },
    [preferences],
  );

  return { preferences, loading, error, updatePreference };
}
