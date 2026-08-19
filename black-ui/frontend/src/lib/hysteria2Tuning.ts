export const HYSTERIA2_DEFAULTS = {
  congestionMode: "standard",
  minAckRate: "0.8",
  maxQueueDelayMs: "80",
  pacingGain: "1.25",
  endpointShards: "1",
  quicEndpoints: "1",
  quicBufferBytes: "8388608",
  datagramPolicy: "standard",
  fecMode: "off"
} as const;

export interface Hysteria2TuningState {
  hysteria2EndpointShards?: string;
  hysteria2CongestionMode: string;
  hysteria2MinAckRate: string;
  hysteria2MaxQueueDelayMs: string;
  hysteria2PacingGain: string;
  hysteria2LossCompensation: boolean;
  hysteria2QuicReusePort: boolean;
  hysteria2QuicEndpoints: string;
  hysteria2QuicRecvBufferBytes: string;
  hysteria2QuicSendBufferBytes: string;
  hysteria2DatagramEnabled: boolean;
  hysteria2DatagramUdpOverDatagram: boolean;
  hysteria2DatagramPolicy: string;
  hysteria2FecMode: string;
  hysteria2FecMaxOverheadPercent: string;
}

export const HYSTERIA2_SIMPLE_CONGESTION_MODES = new Set(["standard", "brutal-compatible", "badnet-low-latency"]);

export function hasCustomHysteria2Congestion(state: Hysteria2TuningState): boolean {
  return (
    !HYSTERIA2_SIMPLE_CONGESTION_MODES.has(state.hysteria2CongestionMode) ||
    state.hysteria2MinAckRate !== HYSTERIA2_DEFAULTS.minAckRate ||
    state.hysteria2MaxQueueDelayMs !== HYSTERIA2_DEFAULTS.maxQueueDelayMs ||
    state.hysteria2PacingGain !== HYSTERIA2_DEFAULTS.pacingGain ||
    !state.hysteria2LossCompensation
  );
}

export function hasCustomHysteria2TransportTuning(
  state: Hysteria2TuningState,
  endpointShards = state.hysteria2EndpointShards
): boolean {
  return (
    (endpointShards !== undefined && endpointShards !== HYSTERIA2_DEFAULTS.endpointShards) ||
    state.hysteria2QuicReusePort ||
    state.hysteria2QuicEndpoints !== HYSTERIA2_DEFAULTS.quicEndpoints ||
    state.hysteria2QuicRecvBufferBytes !== HYSTERIA2_DEFAULTS.quicBufferBytes ||
    state.hysteria2QuicSendBufferBytes !== HYSTERIA2_DEFAULTS.quicBufferBytes ||
    state.hysteria2DatagramEnabled ||
    !state.hysteria2DatagramUdpOverDatagram ||
    state.hysteria2DatagramPolicy !== HYSTERIA2_DEFAULTS.datagramPolicy ||
    state.hysteria2FecMode !== HYSTERIA2_DEFAULTS.fecMode ||
    state.hysteria2FecMaxOverheadPercent.trim() !== ""
  );
}

export function hasCustomHysteria2Tuning(state: Hysteria2TuningState, endpointShards?: string): boolean {
  return hasCustomHysteria2Congestion(state) || hasCustomHysteria2TransportTuning(state, endpointShards);
}

export function defaultHysteria2TransportTuning() {
  return {
    hysteria2EndpointShards: HYSTERIA2_DEFAULTS.endpointShards,
    hysteria2QuicReusePort: false,
    hysteria2QuicEndpoints: HYSTERIA2_DEFAULTS.quicEndpoints,
    hysteria2QuicRecvBufferBytes: HYSTERIA2_DEFAULTS.quicBufferBytes,
    hysteria2QuicSendBufferBytes: HYSTERIA2_DEFAULTS.quicBufferBytes,
    hysteria2DatagramEnabled: false,
    hysteria2DatagramUdpOverDatagram: true,
    hysteria2DatagramPolicy: HYSTERIA2_DEFAULTS.datagramPolicy,
    hysteria2FecMode: HYSTERIA2_DEFAULTS.fecMode,
    hysteria2FecMaxOverheadPercent: ""
  } satisfies Partial<Hysteria2TuningState>;
}

export function defaultHysteria2CongestionTuning(mode: string = HYSTERIA2_DEFAULTS.congestionMode) {
  return {
    hysteria2CongestionMode: mode,
    hysteria2MinAckRate: HYSTERIA2_DEFAULTS.minAckRate,
    hysteria2MaxQueueDelayMs: HYSTERIA2_DEFAULTS.maxQueueDelayMs,
    hysteria2PacingGain: HYSTERIA2_DEFAULTS.pacingGain,
    hysteria2LossCompensation: true
  } satisfies Partial<Hysteria2TuningState>;
}
