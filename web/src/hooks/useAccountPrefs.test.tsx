import { act, renderHook } from "@testing-library/react";
import { useAccountPrefs } from "./useAccountPrefs";

describe("useAccountPrefs", () => {
  it("persists the last credential source per selected key", () => {
    const { result, unmount } = renderHook(() => useAccountPrefs("sign"));

    act(() => {
      result.current.setLastSource("alice.near", "ed25519:alice", "nearxd_keychain");
    });

    expect(result.current.lastCredentialSource).toBe("nearxd_keychain");
    expect(result.current.getLastSource("alice.near", "ed25519:alice")).toBe(
      "nearxd_keychain",
    );
    expect(result.current.getLastSource("alice.near", "ed25519:bob")).toBeNull();
    unmount();

    const { result: nextResult } = renderHook(() => useAccountPrefs("sign"));
    expect(nextResult.current.getLastSource("alice.near", "ed25519:alice")).toBe(
      "nearxd_keychain",
    );
  });
});
