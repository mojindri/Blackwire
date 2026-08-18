import { afterEach, describe, expect, it, vi } from "vitest";
import { fetchSubscriptionContent, MAX_SUBSCRIPTION_QR_BYTES, subscriptionQrPayload } from "./subscription";

describe("fetchSubscriptionContent", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("returns the fetched subscription body", async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      text: async () => "dmxlc3M6Ly9leGFtcGxl"
    });
    vi.stubGlobal("fetch", fetchMock);

    await expect(fetchSubscriptionContent("http://panel/sub/token")).resolves.toEqual({
      ok: true,
      content: "dmxlc3M6Ly9leGFtcGxl",
      message: "Copied"
    });
    expect(fetchMock).toHaveBeenCalledWith("http://panel/sub/token", { cache: "no-store" });
  });

  it("rejects empty subscription bodies", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: true,
        text: async () => " \n"
      })
    );

    await expect(fetchSubscriptionContent("http://panel/sub/token")).resolves.toEqual({
      ok: false,
      content: "",
      message: "Subscription is empty"
    });
  });

  it("reports HTTP errors without returning content", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: false,
        status: 404,
        text: async () => "not found"
      })
    );

    await expect(fetchSubscriptionContent("http://panel/sub/token")).resolves.toEqual({
      ok: false,
      content: "",
      message: "Subscription returned 404"
    });
  });

  it("prepares trimmed subscription content for a scannable QR code", () => {
    expect(subscriptionQrPayload("  vless://example\n")).toEqual({
      ok: true,
      content: "vless://example",
      bytes: 15
    });
  });

  it("rejects QR payloads that would be too dense to scan reliably", () => {
    const result = subscriptionQrPayload("x".repeat(MAX_SUBSCRIPTION_QR_BYTES + 1));
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.message).toContain("reliably scannable QR code");
  });
});
