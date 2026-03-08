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
  selectionRequiredLabel?: string;
  selectionRequiredMessage?: string | null;
  sourceNeededLabel?: string;
  sourceNeededMessage?: string | null;
  incompatibleLabel?: string;
  incompatibleMessage?: string | null;
  advisoryLabel?: string;
  advisoryMessage?: string | null;
  advisoryTone?: SignerSummaryTone;
  readyLabel: string;
  readyMessage: string;
}

export function resolveSignerSummaryStatus({
  hardwareError,
  error,
  neutralLabel = "Choose signer",
  neutralMessage,
  selectionRequiredLabel = "Signer required",
  selectionRequiredMessage,
  sourceNeededLabel = "Source needed",
  sourceNeededMessage,
  incompatibleLabel = "Signer needs attention",
  incompatibleMessage,
  advisoryLabel = "Heads up",
  advisoryMessage,
  advisoryTone = "warning",
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
      label: "Action blocked",
      tone: "warning",
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

  if (selectionRequiredMessage) {
    return {
      label: selectionRequiredLabel,
      tone: "warning",
      message: selectionRequiredMessage,
    };
  }

  if (sourceNeededMessage) {
    return {
      label: sourceNeededLabel,
      tone: "warning",
      message: sourceNeededMessage,
    };
  }

  if (incompatibleMessage) {
    return {
      label: incompatibleLabel,
      tone: "warning",
      message: incompatibleMessage,
    };
  }

  if (advisoryMessage) {
    return {
      label: advisoryLabel,
      tone: advisoryTone,
      message: advisoryMessage,
    };
  }

  return {
    label: readyLabel,
    tone: "success",
    message: readyMessage,
  };
}
