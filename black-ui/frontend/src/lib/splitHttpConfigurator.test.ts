import { describe, expect, it } from "vitest";
import { buildSplitHttp, readSplitHttp } from "./splitHttpConfigurator";

describe("SplitHTTP typed controls", () => {
  it("round-trips the full XHTTP surface without raw JSON editing", () => {
    const source = {
      path: "/packet", host: ["edge.example.com"], method: "POST", mode: "packet-up", uplinkHTTPMethod: "PUT",
      headers: { Host: "cdn.example.com" }, xPaddingBytes: { min: 10, max: 50 }, xPaddingMethod: "tokenish",
      xPaddingHeader: "X-Pad", xPaddingKey: "pad", xPaddingPlacement: "header", sessionPlacement: "cookie",
      sessionKey: "sid", seqPlacement: "query", seqKey: "seq", uplinkDataPlacement: "body", uplinkDataKey: "data",
      uplinkChunkSize: 65536, scMaxBufferedPosts: 8,
      xmux: { maxConcurrency: 8, maxConnections: 4, cMaxReuseTimes: 32, hMaxRequestTimes: 64, hMaxReusableSecs: 300, hKeepAlivePeriod: 30 },
      downloadSettings: { network: "grpc", security: "tls" }
    };
    expect(buildSplitHttp(readSplitHttp(source), {})).toEqual(source);
  });

  it("supports fixed and range padding forms", () => {
    expect(buildSplitHttp(readSplitHttp({ path: "/", xPaddingBytes: 512 }), {}).xPaddingBytes).toBe(512);
    expect(buildSplitHttp(readSplitHttp({ path: "/", xPaddingBytes: "100-900" }), {}).xPaddingBytes).toBe("100-900");
  });

  it("preserves explicitly enabled empty Xmux and download settings", () => {
    const state = readSplitHttp({ xmux: {}, downloadSettings: {} });
    expect(state.xmuxEnabled).toBe(true);
    expect(state.downloadEnabled).toBe(true);
    expect(buildSplitHttp(state, {})).toMatchObject({ xmux: {}, downloadSettings: {} });
  });
});
