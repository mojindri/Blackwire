import { describe, expect, it } from "vitest";
import { defaultHysteria2CongestionTuning, defaultHysteria2TransportTuning, hasCustomHysteria2Congestion, hasCustomHysteria2TransportTuning, hasCustomHysteria2Tuning, HYSTERIA2_DEFAULTS } from "./hysteria2Tuning";

const defaults = {
  hysteria2CongestionMode: HYSTERIA2_DEFAULTS.congestionMode,
  hysteria2MinAckRate: HYSTERIA2_DEFAULTS.minAckRate,
  hysteria2MaxQueueDelayMs: HYSTERIA2_DEFAULTS.maxQueueDelayMs,
  hysteria2PacingGain: HYSTERIA2_DEFAULTS.pacingGain,
  hysteria2LossCompensation: true,
  hysteria2QuicReusePort: false,
  hysteria2QuicEndpoints: HYSTERIA2_DEFAULTS.quicEndpoints,
  hysteria2QuicRecvBufferBytes: HYSTERIA2_DEFAULTS.quicBufferBytes,
  hysteria2QuicSendBufferBytes: HYSTERIA2_DEFAULTS.quicBufferBytes
};

describe("Hysteria2 custom tuning detection", () => {
  it("keeps the standard presets simple", () => {
    expect(hasCustomHysteria2Tuning(defaults)).toBe(false);
    expect(hasCustomHysteria2Tuning({ ...defaults, hysteria2CongestionMode: "brutal-compatible" })).toBe(false);
    expect(hasCustomHysteria2Tuning({ ...defaults, hysteria2CongestionMode: "badnet-low-latency" })).toBe(false);
  });

  it("detects shared and outbound-only overrides", () => {
    expect(hasCustomHysteria2Tuning({ ...defaults, hysteria2QuicRecvBufferBytes: "16777216" })).toBe(true);
    expect(hasCustomHysteria2Tuning(defaults, "2")).toBe(true);
    expect(hasCustomHysteria2Tuning(defaults, HYSTERIA2_DEFAULTS.endpointShards)).toBe(false);
  });

  it("separates congestion choices from endpoint transport overrides", () => {
    expect(hasCustomHysteria2Congestion({ ...defaults, hysteria2CongestionMode: "nova-cc" })).toBe(true);
    expect(hasCustomHysteria2TransportTuning({ ...defaults, hysteria2CongestionMode: "nova-cc" })).toBe(false);
    expect(defaultHysteria2TransportTuning()).toMatchObject({
      hysteria2QuicRecvBufferBytes: "8388608"
    });
    expect(defaultHysteria2CongestionTuning("brutal-compatible")).toMatchObject({
      hysteria2CongestionMode: "brutal-compatible",
      hysteria2MinAckRate: "0.8",
      hysteria2LossCompensation: true
    });
  });
});
