import { summarizeBroadcastResult } from "./broadcastSummary";

describe("summarizeBroadcastResult", () => {
  it("extracts tx hash and success from a successful RPC result", () => {
    const summary = summarizeBroadcastResult(
      {
        final_execution_status: "SUCCESS_VALUE",
        transaction: {
          hash: "7cCD8EwDnMT5successhash",
        },
      },
      "fallback-hash",
    );

    expect(summary).toEqual({
      txHash: "7cCD8EwDnMT5successhash",
      success: true,
      statusLabel: "SUCCESS_VALUE",
    });
  });

  it("falls back to the signed tx hash on broadcast failure summaries", () => {
    const summary = summarizeBroadcastResult(
      {
        final_execution_status: "FAILURE",
      },
      "signed-hash-fallback",
    );

    expect(summary).toEqual({
      txHash: "signed-hash-fallback",
      success: false,
      statusLabel: "FAILURE",
    });
  });
});
