import { describe, expect, it } from "vitest";
import { MAX_SUBSCRIPTION_QR_BYTES, subscriptionQrPayload } from "./subscription";

describe("subscriptionQrPayload", () => {
  it("prepares a trimmed public subscription URL for a scannable QR code", () => {
    expect(subscriptionQrPayload("  https://panel.example/sub/token\n")).toEqual({
      ok: true,
      content: "https://panel.example/sub/token",
      bytes: 31
    });
  });

  it("rejects proxy links and malformed URLs", () => {
    expect(subscriptionQrPayload("vless://example")).toEqual({
      ok: false,
      message: "Subscription URL must use HTTP or HTTPS"
    });
    expect(subscriptionQrPayload("not a URL")).toEqual({
      ok: false,
      message: "Subscription URL is invalid"
    });
  });

  it("rejects QR payloads that would be too dense to scan reliably", () => {
    const result = subscriptionQrPayload(
      `https://panel.example/sub/${"x".repeat(MAX_SUBSCRIPTION_QR_BYTES)}`
    );
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.message).toContain("reliably scannable QR code");
  });
});
