import type { CoreSettings } from "./types";

export type OptimizationMode = "automatic" | "compatibility";

export const defaultFastSettings: NonNullable<CoreSettings["fast"]> = {
  strictProduction: true,
  pool: "adaptive",
  splice: "adaptive",
  relay: {
    engine: "v2",
    flush: "adaptive",
    initialBuffer: 16_384,
    maxBuffer: 262_144
  },
  linux: {
    zerocopy: "disabled",
    zerocopyMinBytes: 16_384,
    ioUring: "disabled"
  }
};

export function optimizationModeFromSettings(settings: CoreSettings): OptimizationMode {
  return settings.profile === "fast" ? "automatic" : "compatibility";
}

export function optimizationStatusFromSettings(settings: CoreSettings): { label: string; detail: string } {
  const mode = optimizationModeFromSettings(settings);
  const vision = settings.vision !== null ? " · Vision override preserved" : "";
  const overrides = settings.fast !== null || (settings.firstPacketBoost?.enabled ?? false)
    ? " · expert overrides active"
    : "";
  if (mode === "automatic") {
    return { label: "Blackwire managed", detail: `Fast profile defaults · adaptive pooling and splice · relay v2${overrides}${vision}` };
  }
  return { label: "Compatibility focused", detail: `Portable relay defaults${overrides}${vision}` };
}

export function applyOptimizationMode(settings: CoreSettings, mode: OptimizationMode): CoreSettings {
  if (mode === "automatic") {
    return {
      ...settings,
      profile: "fast",
      fast: null,
      firstPacketBoost: null
    };
  }

  return {
    ...settings,
    profile: "compat",
    fast: null,
    firstPacketBoost: null
  };
}
