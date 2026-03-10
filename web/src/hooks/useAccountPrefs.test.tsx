import { act, renderHook } from "@testing-library/react";
import { useAccountPrefs } from "./useAccountPrefs";

describe("useAccountPrefs", () => {
  it("persists the last selected account and key", () => {
    const { result, unmount } = renderHook(() => useAccountPrefs("sign"));

    act(() => {
      result.current.setLastAccount("alice.near");
      result.current.setLastKey("ed25519:alice");
    });

    expect(result.current.lastAccountId).toBe("alice.near");
    expect(result.current.lastPublicKey).toBe("ed25519:alice");
    unmount();

    const { result: nextResult } = renderHook(() => useAccountPrefs("sign"));
    expect(nextResult.current.lastAccountId).toBe("alice.near");
    expect(nextResult.current.lastPublicKey).toBe("ed25519:alice");
  });
});
