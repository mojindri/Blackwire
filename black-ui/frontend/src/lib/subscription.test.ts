import { afterEach, describe, expect, it, vi } from "vitest";
import { fetchSubscriptionContent } from "./subscription";

describe("fetchSubscriptionContent", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("returns the fetched subscription body", async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      text: async () => "vless://example"
    });
    vi.stubGlobal("fetch", fetchMock);

    await expect(fetchSubscriptionContent("http://panel/sub/token")).resolves.toEqual({
      ok: true,
      content: "vless://example",
      message: "Copied"
    });
    expect(fetchMock).toHaveBeenCalledWith("http://panel/sub/token/raw", { cache: "no-store" });
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
});
