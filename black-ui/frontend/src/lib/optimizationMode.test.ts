import { describe, expect, it } from "vitest";
import { applyOptimizationMode, defaultFastSettings, optimizationModeFromSettings, optimizationStatusFromSettings } from "./optimizationMode";
import type { CoreSettings } from "./types";

function settings(overrides: Partial<CoreSettings> = {}): CoreSettings {
  return {
    profile: "compat",
    fast: null,
    vision: null,
    firstPacketBoost: null,
    metricsAddr: null,
    stats: null,
    limits: {
      maxConnections: null,
      maxConnectionsPerInbound: null,
      maxConnectionsPerUser: null,
      maxHandshakeSeconds: null,
      maxIdleSeconds: null
    },
    quic: null,
    ...overrides
  };
}

describe("optimization mode", () => {
  it("recognizes automatic and compatibility configurations", () => {
    expect(optimizationModeFromSettings(settings({ profile: "fast" }))).toBe("automatic");
    expect(optimizationModeFromSettings(settings())).toBe("compatibility");
  });

  it("keeps explicit policies within the selected managed mode", () => {
    expect(optimizationModeFromSettings(settings({ profile: "fast", fast: defaultFastSettings }))).toBe("automatic");
  });

  it("ignores a legacy disabled boost object", () => {
    expect(optimizationModeFromSettings(settings({
      profile: "fast",
      firstPacketBoost: {
        enabled: false,
        dns: true,
        sendEarlyPayload: true,
      }
    }))).toBe("automatic");
  });

  it("applies automatic mode without persisting duplicate defaults", () => {
    const result = applyOptimizationMode(settings({
      profile: "compat",
      fast: defaultFastSettings,
      firstPacketBoost: {
        enabled: true,
        dns: true,
        sendEarlyPayload: true,
      }
    }), "automatic");

    expect(result.profile).toBe("fast");
    expect(result.fast).toBeNull();
    expect(result.firstPacketBoost).toBeNull();
  });

  it("does not alter protocol-specific Vision settings", () => {
    const vision: NonNullable<CoreSettings["vision"]> = {
      directCopy: "auto",
      maxPacketsToFilter: 8,
      allowSpliceAfterDirect: true
    };
    expect(applyOptimizationMode(settings({ vision }), "automatic").vision).toEqual(vision);
    expect(optimizationStatusFromSettings(settings({ profile: "fast", vision })).detail).toContain("Vision override preserved");
    expect(optimizationStatusFromSettings(settings({ vision })).detail).toContain("Vision override preserved");
  });
});
