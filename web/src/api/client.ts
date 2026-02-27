import { apiBaseUrl } from "../config";
import { getFastnearApiUrl } from "../tauri/runtime";

export async function fetchApi<T>(
  endpoint: string,
  body: Record<string, unknown> = {},
): Promise<T> {
  const baseUrl = await getFastnearApiUrl(apiBaseUrl);

  const res = await fetch(`${baseUrl}/v0/${endpoint}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    throw new Error(`API error ${res.status}: ${res.statusText}`);
  }
  return res.json();
}
