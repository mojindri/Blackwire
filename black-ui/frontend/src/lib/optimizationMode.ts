import type { CoreSettings } from "./types";

export type OptimizationMode = "automatic" | "compatibility" | "custom";

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
  const hasManualPerformancePolicy = settings.fast !== null
    || settings.budget !== null
    || (settings.firstPacketBoost?.enabled ?? false);

  if (!hasManualPerformancePolicy && settings.profile === "fast") return "automatic";
  if (!hasManualPerformancePolicy && settings.profile === "compat") return "compatibility";
  return "custom";
}

export function optimizationStatusFromSettings(settings: CoreSettings): { label: string; detail: string } {
  const mode = optimizationModeFromSettings(settings);
  const vision = settings.vision !== null ? " · Vision override preserved" : "";
  if (mode === "automatic") {
    return { label: "Blackwire managed", detail: `Fast profile defaults · adaptive pooling and splice · relay v2${vision}` };
  }
  if (mode === "compatibility") {
    return { label: "Compatibility focused", detail: `Portable relay defaults${vision}` };
  }
  return { label: "Operator managed", detail: `Profile: ${settings.profile} · explicit policies preserved` };
}

export function applyOptimizationMode(settings: CoreSettings, mode: OptimizationMode): CoreSettings {
  if (mode === "automatic") {
    return {
      ...settings,
      profile: "fast",
      fast: null,
      budget: null,
      firstPacketBoost: null
    };
  }

  if (mode === "compatibility") {
    return {
      ...settings,
      profile: "compat",
      fast: null,
      budget: null,
      firstPacketBoost: null
    };
  }

  if (optimizationModeFromSettings(settings) === "custom") return settings;

  return {
    ...settings,
    profile: "fast",
    fast: {
      ...defaultFastSettings,
      relay: { ...defaultFastSettings.relay },
      linux: { ...defaultFastSettings.linux }
    }
  };
}
