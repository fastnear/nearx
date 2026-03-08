export class RetryableRequestError extends Error {
  status?: number;

  constructor(message: string, status?: number) {
    super(message);
    this.name = "RetryableRequestError";
    this.status = status;
  }
}

export class RequestTimeoutError extends RetryableRequestError {
  constructor(timeoutMs: number) {
    super(`request timed out after ${timeoutMs}ms`);
    this.name = "RequestTimeoutError";
  }
}

export interface RetryAsyncOptions<T> {
  retries?: number;
  baseDelayMs?: number;
  maxDelayMs?: number;
  shouldRetryResult?: (result: T) => boolean;
  shouldRetryError?: (error: unknown) => boolean;
}

const DEFAULT_RETRIES = 2;
const DEFAULT_BASE_DELAY_MS = 250;
const DEFAULT_MAX_DELAY_MS = 1_000;

export function isRetryableHttpStatus(status: number): boolean {
  return status === 408 || status === 425 || status === 429 || (status >= 500 && status <= 504);
}

export function isRetryableMessage(message: string): boolean {
  const normalized = message.trim().toLowerCase();
  return (
    normalized.includes("timeout") ||
    normalized.includes("timed out") ||
    normalized.includes("too many requests") ||
    normalized.includes("rate limit") ||
    normalized.includes("temporarily unavailable") ||
    normalized.includes("connection reset") ||
    normalized.includes("connection refused") ||
    normalized.includes("network error") ||
    normalized.includes("failed to fetch") ||
    normalized.includes("load failed")
  );
}

export function isRetryableError(error: unknown): boolean {
  if (error instanceof RetryableRequestError) {
    return true;
  }
  if (error instanceof TypeError) {
    return true;
  }
  if (error instanceof Error) {
    return isRetryableMessage(error.message);
  }
  return false;
}

export async function retryAsync<T>(
  task: () => Promise<T>,
  options: RetryAsyncOptions<T> = {},
): Promise<T> {
  const retries = options.retries ?? DEFAULT_RETRIES;
  const baseDelayMs = options.baseDelayMs ?? DEFAULT_BASE_DELAY_MS;
  const maxDelayMs = options.maxDelayMs ?? DEFAULT_MAX_DELAY_MS;
  const shouldRetryError = options.shouldRetryError ?? isRetryableError;

  for (let attempt = 0; ; attempt += 1) {
    try {
      const result = await task();
      if (
        options.shouldRetryResult?.(result) &&
        attempt < retries
      ) {
        await delay(backoffDelay(attempt, baseDelayMs, maxDelayMs));
        continue;
      }
      return result;
    } catch (error) {
      if (attempt >= retries || !shouldRetryError(error)) {
        throw error;
      }
      await delay(backoffDelay(attempt, baseDelayMs, maxDelayMs));
    }
  }
}

export function linkAbortSignals(
  parentSignal: AbortSignal | undefined,
  childController: AbortController,
): () => void {
  if (!parentSignal) {
    return () => {};
  }
  if (parentSignal.aborted) {
    childController.abort(parentSignal.reason);
    return () => {};
  }
  const abort = () => childController.abort(parentSignal.reason);
  parentSignal.addEventListener("abort", abort, { once: true });
  return () => parentSignal.removeEventListener("abort", abort);
}

function backoffDelay(
  attempt: number,
  baseDelayMs: number,
  maxDelayMs: number,
): number {
  return Math.min(baseDelayMs * 2 ** attempt, maxDelayMs);
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}
