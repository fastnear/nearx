import { resolveSignerSummaryStatus } from "./signerSummaryStatus";

describe("resolveSignerSummaryStatus", () => {
  it("prioritizes ledger and blocking errors ahead of ready state", () => {
    const status = resolveSignerSummaryStatus({
      hardwareError: "Reconnect Ledger.",
      error: "Blocked.",
      selectionRequiredMessage: "Pick a signer.",
      readyLabel: "Ready",
      readyMessage: "All set.",
    });

    expect(status).toEqual({
      label: "Ledger attention",
      tone: "warning",
      message: "Reconnect Ledger.",
    });
  });

  it("returns the ready state when no blocking condition remains", () => {
    const status = resolveSignerSummaryStatus({
      readyLabel: "Ready to sign",
      readyMessage: "Signer is ready.",
    });

    expect(status).toEqual({
      label: "Ready to sign",
      tone: "success",
      message: "Signer is ready.",
    });
  });

  it("supports advisory warnings without treating them as hard errors", () => {
    const status = resolveSignerSummaryStatus({
      advisoryLabel: "Fingerprint off",
      advisoryMessage: "Fingerprint verification is not used for File system.",
      readyLabel: "Ready",
      readyMessage: "All set.",
    });

    expect(status).toEqual({
      label: "Fingerprint off",
      tone: "warning",
      message: "Fingerprint verification is not used for File system.",
    });
  });
});
