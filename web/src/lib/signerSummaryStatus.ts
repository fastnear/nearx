export type SignerSummaryTone = "neutral" | "success" | "warning" | "danger";

export interface SignerSummaryStatus {
  label: string;
  tone: SignerSummaryTone;
  message: string;
}

interface ResolveSignerSummaryStatusOptions {
  hardwareError?: string | null;
  error?: string | null;
  neutralLabel?: string;
  neutralMessage?: string | null;
  readyLabel: string;
  readyMessage: string;
}

export function resolveSignerSummaryStatus({
  hardwareError,
  error,
  neutralLabel = "Choose signer",
  neutralMessage,
  readyLabel,
  readyMessage,
}: ResolveSignerSummaryStatusOptions): SignerSummaryStatus {
  if (hardwareError) {
    return {
      label: "Ledger attention",
      tone: "warning",
      message: hardwareError,
    };
  }

  if (error) {
    return {
      label: "Error",
      tone: "danger",
      message: error,
    };
  }

  if (neutralMessage) {
    return {
      label: neutralLabel,
      tone: "neutral",
      message: neutralMessage,
    };
  }

  return {
    label: readyLabel,
    tone: "success",
    message: readyMessage,
  };
}
