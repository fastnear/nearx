function asRecord(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === "object" ? (value as Record<string, unknown>) : null;
}

function getString(value: unknown, path: string[]): string | null {
  let current: unknown = value;
  for (const segment of path) {
    const record = asRecord(current);
    if (!record) {
      return null;
    }
    current = record[segment];
  }
  return typeof current === "string" && current.trim() ? current : null;
}

function inferSuccessFromOutcomeStatus(value: unknown): boolean | null {
  const record = asRecord(value);
  if (!record) {
    return null;
  }
  if ("Failure" in record) {
    return false;
  }
  if ("SuccessValue" in record || "SuccessReceiptId" in record) {
    return true;
  }
  return null;
}

export interface BroadcastSummary {
  txHash: string;
  success: boolean | null;
  statusLabel: string;
}

export function summarizeBroadcastResult(
  result: unknown,
  fallbackTxHash: string,
): BroadcastSummary {
  const txHash =
    getString(result, ["transaction", "hash"]) ??
    getString(result, ["transaction_outcome", "id"]) ??
    fallbackTxHash;

  const finalExecutionStatus = getString(result, ["final_execution_status"]);
  if (finalExecutionStatus) {
    return {
      txHash,
      success:
        finalExecutionStatus === "FAILURE"
          ? false
          : finalExecutionStatus.startsWith("SUCCESS")
            ? true
            : null,
      statusLabel: finalExecutionStatus,
    };
  }

  const transactionOutcomeStatus = inferSuccessFromOutcomeStatus(
    asRecord(result)?.transaction_outcome &&
      asRecord(asRecord(result)?.transaction_outcome)?.outcome &&
      asRecord(asRecord(asRecord(result)?.transaction_outcome)?.outcome)?.status,
  );

  return {
    txHash,
    success: transactionOutcomeStatus,
    statusLabel:
      transactionOutcomeStatus === true
        ? "SUCCESS"
        : transactionOutcomeStatus === false
          ? "FAILURE"
          : "UNKNOWN",
  };
}
