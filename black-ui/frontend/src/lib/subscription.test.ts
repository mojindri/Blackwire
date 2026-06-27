import { afterEach, describe, expect, it, vi } from "vitest";
import { fetchSubscriptionContent } from "./subscription";

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
});
