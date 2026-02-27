import { invoke } from "@tauri-apps/api/core";
import { getCurrent, onOpenUrl } from "@tauri-apps/plugin-deep-link";

export interface RuntimeConfig {
  near_node_url: string;
  fastnear_api_url: string;
  fastnear_auth_token: string | null;
  fastnear_auth_token_source: string;
  broker_available: boolean;
}

let runtimeConfigPromise: Promise<RuntimeConfig | null> | null = null;

export function isTauriRuntime(): boolean {
  return (
    import.meta.env.VITE_TAURI === "true" ||
    window.location.protocol === "tauri:"
  );
}

export async function getRuntimeConfig(): Promise<RuntimeConfig | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  try {
    return await invoke<RuntimeConfig>("get_runtime_config");
  } catch {
    return null;
  }
}

export async function getRuntimeConfigCached(): Promise<RuntimeConfig | null> {
  if (!runtimeConfigPromise) {
    runtimeConfigPromise = getRuntimeConfig();
  }
  return runtimeConfigPromise;
}

export async function getFastnearApiUrl(defaultUrl: string): Promise<string> {
  const cfg = await getRuntimeConfigCached();
  return cfg?.fastnear_api_url ?? defaultUrl;
}

export async function getNearNodeUrl(defaultUrl: string): Promise<string> {
  const cfg = await getRuntimeConfigCached();
  return cfg?.near_node_url ?? defaultUrl;
}

export async function openExternal(url: string): Promise<void> {
  if (isTauriRuntime()) {
    try {
      await invoke("open_external", { url });
      return;
    } catch {
      // Fallback below
    }
  }

  window.open(url, "_blank", "noopener,noreferrer");
}

export async function subscribeDeepLinks(
  onUrl: (url: string) => void,
): Promise<() => void> {
  if (!isTauriRuntime()) {
    return () => {};
  }

  try {
    const current = (await getCurrent()) ?? [];
    for (const url of current) {
      onUrl(url);
    }

    return await onOpenUrl((urls) => {
      for (const url of urls) {
        onUrl(url);
      }
    });
  } catch {
    return () => {};
  }
}
