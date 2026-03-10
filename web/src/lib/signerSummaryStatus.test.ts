import { resolveSignerSummaryStatus } from "./signerSummaryStatus";

describe("resolveSignerSummaryStatus", () => {
  it("prioritizes ledger errors ahead of ready state", () => {
    const status = resolveSignerSummaryStatus({
      hardwareError: "Reconnect Ledger.",
      error: "Blocked.",
      readyLabel: "Ready",
      readyMessage: "All set.",
    });

    expect(status).toEqual({
      label: "Ledger attention",
      tone: "warning",
      message: "Reconnect Ledger.",
    });
  });

  it("shows error when present", () => {
    const status = resolveSignerSummaryStatus({
      error: "Something went wrong.",
      readyLabel: "Ready",
      readyMessage: "All set.",
    });

    expect(status).toEqual({
      label: "Error",
      tone: "danger",
      message: "Something went wrong.",
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

  it("shows neutral state when neutral message is set", () => {
    const status = resolveSignerSummaryStatus({
      neutralMessage: "Select an account to get started.",
      readyLabel: "Ready",
      readyMessage: "All set.",
    });

    expect(status).toEqual({
      label: "Choose signer",
      tone: "neutral",
      message: "Select an account to get started.",
    });
  });
});
