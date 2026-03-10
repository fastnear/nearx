import {
  RequestTimeoutError,
  RetryableRequestError,
  isRetryableError,
  isRetryableHttpStatus,
  retryAsync,
} from "./retry";

describe("retryAsync", () => {
  it("retries retryable errors and eventually resolves", async () => {
    let attempts = 0;
    const result = await retryAsync(async () => {
      attempts += 1;
      if (attempts < 3) {
        throw new RetryableRequestError("Too many requests", 429);
      }
      return "ok";
    });

    expect(result).toBe("ok");
    expect(attempts).toBe(3);
  });

  it("does not retry non-retryable errors", async () => {
    await expect(
      retryAsync(async () => {
        throw new Error("permission denied");
      }),
    ).rejects.toThrow("permission denied");
  });
});

describe("retry classifiers", () => {
  it("marks transient HTTP statuses as retryable", () => {
    expect(isRetryableHttpStatus(429)).toBe(true);
    expect(isRetryableHttpStatus(503)).toBe(true);
    expect(isRetryableHttpStatus(404)).toBe(false);
  });

  it("marks timeout/network errors as retryable", () => {
    expect(isRetryableError(new RequestTimeoutError(5000))).toBe(true);
    expect(isRetryableError(new TypeError("Failed to fetch"))).toBe(true);
    expect(isRetryableError(new Error("rate limit exceeded"))).toBe(true);
    expect(isRetryableError(new Error("invalid account"))).toBe(false);
  });
});
