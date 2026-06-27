import type { Outbound, OutboundInput } from "./types";

export type JsonObject = Record<string, unknown>;
export type OutboundSliceKey = "settings" | "streamSettings";

export interface SliceState {
  text: string;
  value: JsonObject;
  error: string;
}

export interface OutboundEditorState {
  tag: string;
  protocol: string;
  enabled: boolean;
  network: string;
  security: string;
  address: string;
  port: string;
  userId: string;
  password: string;
  method: string;
  server: string;
  hysteria2Auth: string;
  hysteria2ServerName: string;
  hysteria2SkipCertVerify: boolean;
  hysteria2EndpointShards: string;
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
  wsPath: string;
  wsHost: string;
  grpcServiceName: string;
  httpupgradePath: string;
  httpupgradeHost: string;
  splitHttpPath: string;
  kcpHeader: string;
  kcpMtu: string;
  kcpTti: string;
  kcpUplinkCapacity: string;
  kcpDownlinkCapacity: string;
  kcpCongestion: boolean;
  kcpReadBufferSize: string;
  kcpWriteBufferSize: string;
  tlsServerName: string;
  tlsAlpn: string;
  tlsCertificateFile: string;
  tlsKeyFile: string;
  realityServerName: string;
  realityPublicKey: string;
  realityShortId: string;
  realityFingerprint: string;
  realitySpiderX: string;
  settings: SliceState;
  streamSettings: SliceState;
}

export interface OutboundValidationIssue {
  field: string;
  message: string;
}

const DEFAULT_PROTOCOL = "freedom";
const DEFAULT_NETWORK = "tcp";
const DEFAULT_SECURITY = "none";
const DEFAULT_HYSTERIA2_CONGESTION_MODE = "standard";
const DEFAULT_HYSTERIA2_MIN_ACK_RATE = "0.8";
const DEFAULT_HYSTERIA2_MAX_QUEUE_DELAY_MS = "80";
const DEFAULT_HYSTERIA2_PACING_GAIN = "1.25";
const DEFAULT_HYSTERIA2_ENDPOINT_SHARDS = "1";
const DEFAULT_HYSTERIA2_QUIC_ENDPOINTS = "1";
const DEFAULT_HYSTERIA2_QUIC_BUFFER_BYTES = "8388608";
const DEFAULT_HYSTERIA2_DATAGRAM_POLICY = "standard";
const DEFAULT_HYSTERIA2_FEC_MODE = "off";

export function createOutboundEditorState(outbound?: Outbound | null): OutboundEditorState {
  const settings = createSliceState(outbound?.settings);
  const streamSettings = createSliceState(outbound?.streamSettings);
  const user0 = firstUser(settings.value);
  const network = stringValue(streamSettings.value.network) ?? DEFAULT_NETWORK;
  const security = stringValue(streamSettings.value.security) ?? DEFAULT_SECURITY;

  return {
    tag: outbound?.tag ?? "freedom",
    protocol: outbound?.protocol ?? DEFAULT_PROTOCOL,
    enabled: outbound?.enabled ?? true,
    network,
    security,
    address: stringValue(settings.value.address) ?? "",
    port: numberString(settings.value.port),
    userId: stringValue(user0?.id) ?? stringValue(settings.value.uuid) ?? "",
    password: stringValue(settings.value.password) ?? "",
    method: stringValue(settings.value.method) ?? "",
    server: stringValue(settings.value.server) ?? "",
    hysteria2Auth: stringValue(settings.value.auth) ?? "",
    hysteria2ServerName: stringValue(settings.value.serverName) ?? "",
    hysteria2SkipCertVerify: boolValue(settings.value.skipCertVerify) ?? false,
    hysteria2EndpointShards: numberString(settings.value.endpointShards) || DEFAULT_HYSTERIA2_ENDPOINT_SHARDS,
    hysteria2CongestionMode: stringValue(objectValue(settings.value.congestion)?.mode) ?? DEFAULT_HYSTERIA2_CONGESTION_MODE,
    hysteria2MinAckRate: numberString(objectValue(settings.value.congestion)?.minAckRate) || DEFAULT_HYSTERIA2_MIN_ACK_RATE,
    hysteria2MaxQueueDelayMs: numberString(objectValue(settings.value.congestion)?.maxQueueDelayMs) || DEFAULT_HYSTERIA2_MAX_QUEUE_DELAY_MS,
    hysteria2PacingGain: numberString(objectValue(settings.value.congestion)?.pacingGain) || DEFAULT_HYSTERIA2_PACING_GAIN,
    hysteria2LossCompensation: boolValue(objectValue(settings.value.congestion)?.lossCompensation) ?? true,
    hysteria2QuicReusePort: boolValue(objectValue(settings.value.quic)?.reusePort) ?? false,
    hysteria2QuicEndpoints: stringOrNumberString(objectValue(settings.value.quic)?.endpoints) || DEFAULT_HYSTERIA2_QUIC_ENDPOINTS,
    hysteria2QuicRecvBufferBytes: numberString(objectValue(settings.value.quic)?.recvBufferBytes) || DEFAULT_HYSTERIA2_QUIC_BUFFER_BYTES,
    hysteria2QuicSendBufferBytes: numberString(objectValue(settings.value.quic)?.sendBufferBytes) || DEFAULT_HYSTERIA2_QUIC_BUFFER_BYTES,
    hysteria2DatagramEnabled: boolValue(objectValue(settings.value.datagram)?.enabled) ?? false,
    hysteria2DatagramUdpOverDatagram: boolValue(objectValue(settings.value.datagram)?.udpOverDatagram) ?? true,
    hysteria2DatagramPolicy: stringValue(objectValue(settings.value.datagram)?.policy) ?? DEFAULT_HYSTERIA2_DATAGRAM_POLICY,
    hysteria2FecMode: stringValue(objectValue(settings.value.fec)?.mode) ?? DEFAULT_HYSTERIA2_FEC_MODE,
    hysteria2FecMaxOverheadPercent: numberString(objectValue(settings.value.fec)?.maxOverheadPercent),
    wsPath: stringValue(objectValue(streamSettings.value.wsSettings)?.path) ?? "",
    wsHost: stringValue(objectValue(objectValue(streamSettings.value.wsSettings)?.headers)?.Host) ?? "",
    grpcServiceName: stringValue(objectValue(streamSettings.value.grpcSettings)?.serviceName) ?? "",
    httpupgradePath: stringValue(objectValue(streamSettings.value.httpupgradeSettings)?.path) ?? "",
    httpupgradeHost: stringValue(objectValue(streamSettings.value.httpupgradeSettings)?.host) ?? "",
    splitHttpPath: stringValue(objectValue(streamSettings.value.splithttpSettings)?.path) ?? "",
    kcpHeader: stringValue(objectValue(streamSettings.value.kcpSettings)?.header) ?? "",
    kcpMtu: numberString(objectValue(streamSettings.value.kcpSettings)?.mtu),
    kcpTti: numberString(objectValue(streamSettings.value.kcpSettings)?.tti),
    kcpUplinkCapacity: numberString(objectValue(streamSettings.value.kcpSettings)?.uplink_capacity),
    kcpDownlinkCapacity: numberString(objectValue(streamSettings.value.kcpSettings)?.downlink_capacity),
    kcpCongestion: boolValue(objectValue(streamSettings.value.kcpSettings)?.congestion) ?? false,
    kcpReadBufferSize: numberString(objectValue(streamSettings.value.kcpSettings)?.read_buffer_size),
    kcpWriteBufferSize: numberString(objectValue(streamSettings.value.kcpSettings)?.write_buffer_size),
    tlsServerName: stringValue(objectValue(streamSettings.value.tlsSettings)?.serverName) ?? "",
    tlsAlpn: arrayOfStrings(objectValue(streamSettings.value.tlsSettings)?.alpn).join(", "),
    tlsCertificateFile: stringValue(objectValue(streamSettings.value.tlsSettings)?.certificateFile) ?? "",
    tlsKeyFile: stringValue(objectValue(streamSettings.value.tlsSettings)?.keyFile) ?? "",
    realityServerName: stringValue(objectValue(streamSettings.value.realitySettings)?.serverName) ?? "",
    realityPublicKey: stringValue(objectValue(streamSettings.value.realitySettings)?.publicKey) ?? "",
    realityShortId:
      stringValue(objectValue(streamSettings.value.realitySettings)?.shortId) ??
      arrayOfStrings(objectValue(streamSettings.value.realitySettings)?.shortIds)[0] ??
      "",
    realityFingerprint: stringValue(objectValue(streamSettings.value.realitySettings)?.fingerprint) ?? "chrome",
    realitySpiderX: stringValue(objectValue(streamSettings.value.realitySettings)?.spiderX) ?? "/",
    settings,
    streamSettings
  };
}

export function buildOutboundInput(state: OutboundEditorState): OutboundInput {
  let settings = cloneObject(state.settings.value);
  let streamSettings = cloneObject(state.streamSettings.value);

  streamSettings = removeTransientNetworkKeys(streamSettings);

  const usesAddressPort = ["vless", "vmess", "trojan", "shadowsocks"].includes(state.protocol);
  if (usesAddressPort) {
    applyStringField(settings, "address", state.address);
    applyNumberField(settings, "port", state.port);
  } else {
    delete settings.address;
    delete settings.port;
  }

  if (state.protocol === "vless" || state.protocol === "vmess") {
    const users = cloneUsers(settings.users);
    const primaryUser = cloneObject(users[0]);
    applyStringField(primaryUser, "id", state.userId);
    users[0] = pruneEmpty(primaryUser);
    settings.users = users.filter((user) => Object.keys(user).length > 0);
    delete settings.password;
    delete settings.method;
    delete settings.server;
  } else {
    delete settings.users;
  }

  if (state.protocol === "trojan" || state.protocol === "shadowsocks" || state.protocol === "tuic") {
    applyStringField(settings, "password", state.password);
  } else {
    delete settings.password;
  }

  if (state.protocol === "shadowsocks") {
    applyStringField(settings, "method", state.method);
  } else {
    delete settings.method;
  }

  if (state.protocol === "hysteria2" || state.protocol === "tuic") {
    applyStringField(settings, "server", state.server);
    delete settings.address;
    delete settings.port;
  } else {
    delete settings.server;
  }

  if (state.protocol === "hysteria2") {
    applyStringField(settings, "auth", state.hysteria2Auth);
    applyStringField(settings, "serverName", state.hysteria2ServerName);
    if (state.hysteria2SkipCertVerify) settings.skipCertVerify = true;
    else delete settings.skipCertVerify;
    applyDefaultedNumberField(settings, "endpointShards", state.hysteria2EndpointShards, DEFAULT_HYSTERIA2_ENDPOINT_SHARDS);
    applyNestedStringField(settings, "congestion", "mode", state.hysteria2CongestionMode, DEFAULT_HYSTERIA2_CONGESTION_MODE);
    applyNestedNumberField(settings, "congestion", "minAckRate", state.hysteria2MinAckRate, DEFAULT_HYSTERIA2_MIN_ACK_RATE);
    applyNestedNumberField(settings, "congestion", "maxQueueDelayMs", state.hysteria2MaxQueueDelayMs, DEFAULT_HYSTERIA2_MAX_QUEUE_DELAY_MS);
    applyNestedNumberField(settings, "congestion", "pacingGain", state.hysteria2PacingGain, DEFAULT_HYSTERIA2_PACING_GAIN);
    applyNestedBoolField(settings, "congestion", "lossCompensation", state.hysteria2LossCompensation, true);
    applyNestedBoolField(settings, "quic", "reusePort", state.hysteria2QuicReusePort, false);
    applyNestedStringOrNumberField(settings, "quic", "endpoints", state.hysteria2QuicEndpoints, DEFAULT_HYSTERIA2_QUIC_ENDPOINTS);
    applyNestedNumberField(settings, "quic", "recvBufferBytes", state.hysteria2QuicRecvBufferBytes, DEFAULT_HYSTERIA2_QUIC_BUFFER_BYTES);
    applyNestedNumberField(settings, "quic", "sendBufferBytes", state.hysteria2QuicSendBufferBytes, DEFAULT_HYSTERIA2_QUIC_BUFFER_BYTES);
    applyNestedBoolField(settings, "datagram", "enabled", state.hysteria2DatagramEnabled, false);
    applyNestedBoolField(settings, "datagram", "udpOverDatagram", state.hysteria2DatagramUdpOverDatagram, true);
    applyNestedStringField(settings, "datagram", "policy", state.hysteria2DatagramPolicy, DEFAULT_HYSTERIA2_DATAGRAM_POLICY);
    applyNestedStringField(settings, "fec", "mode", state.hysteria2FecMode, DEFAULT_HYSTERIA2_FEC_MODE);
    applyNestedNumberField(settings, "fec", "maxOverheadPercent", state.hysteria2FecMaxOverheadPercent);
  } else {
    delete settings.auth;
    delete settings.serverName;
    delete settings.skipCertVerify;
    delete settings.endpointShards;
    delete settings.congestion;
    delete settings.quic;
    delete settings.datagram;
    delete settings.fec;
  }

  if (state.protocol === "tuic") {
    applyStringField(settings, "uuid", state.userId);
    delete settings.method;
  } else {
    delete settings.uuid;
  }

  streamSettings.network = state.network;
  streamSettings.security = state.security;

  if (state.network === "ws") {
    const wsSettings = cloneObject(objectValue(streamSettings.wsSettings));
    wsSettings.path = state.wsPath.trim() || "/";
    const headers = cloneObject(objectValue(wsSettings.headers));
    if (state.wsHost.trim()) headers.Host = state.wsHost.trim();
    else delete headers.Host;
    wsSettings.headers = headers;
    streamSettings.wsSettings = pruneEmpty(wsSettings);
  } else {
    delete streamSettings.wsSettings;
  }

  if (state.network === "grpc") {
    const grpcSettings = cloneObject(objectValue(streamSettings.grpcSettings));
    grpcSettings.serviceName = state.grpcServiceName.trim() || "GunService";
    streamSettings.grpcSettings = pruneEmpty(grpcSettings);
  } else {
    delete streamSettings.grpcSettings;
  }

  if (state.network === "httpupgrade") {
    const httpupgradeSettings = cloneObject(objectValue(streamSettings.httpupgradeSettings));
    httpupgradeSettings.path = state.httpupgradePath.trim() || "/";
    if (state.httpupgradeHost.trim()) httpupgradeSettings.host = state.httpupgradeHost.trim();
    else delete httpupgradeSettings.host;
    streamSettings.httpupgradeSettings = pruneEmpty(httpupgradeSettings);
  } else {
    delete streamSettings.httpupgradeSettings;
  }

  if (state.network === "splithttp") {
    const splitHttpSettings = cloneObject(objectValue(streamSettings.splithttpSettings));
    splitHttpSettings.path = state.splitHttpPath.trim() || "/";
    streamSettings.splithttpSettings = pruneEmpty(splitHttpSettings);
  } else {
    delete streamSettings.splithttpSettings;
  }

  if (state.network === "kcp") {
    const kcpSettings = cloneObject(objectValue(streamSettings.kcpSettings));
    applyStringField(kcpSettings, "header", state.kcpHeader);
    applyNumberField(kcpSettings, "mtu", state.kcpMtu);
    applyNumberField(kcpSettings, "tti", state.kcpTti);
    applyNumberField(kcpSettings, "uplink_capacity", state.kcpUplinkCapacity);
    applyNumberField(kcpSettings, "downlink_capacity", state.kcpDownlinkCapacity);
    if (state.kcpCongestion) {
      kcpSettings.congestion = true;
    } else {
      delete kcpSettings.congestion;
    }
    applyNumberField(kcpSettings, "read_buffer_size", state.kcpReadBufferSize);
    applyNumberField(kcpSettings, "write_buffer_size", state.kcpWriteBufferSize);
    streamSettings.kcpSettings = pruneEmpty(kcpSettings);
  } else {
    delete streamSettings.kcpSettings;
  }

  if (state.security === "tls") {
    const tlsSettings = cloneObject(objectValue(streamSettings.tlsSettings));
    applyStringField(tlsSettings, "serverName", state.tlsServerName);
    applyStringArrayField(tlsSettings, "alpn", state.tlsAlpn);
    applyStringField(tlsSettings, "certificateFile", state.tlsCertificateFile);
    applyStringField(tlsSettings, "keyFile", state.tlsKeyFile);
    streamSettings.tlsSettings = pruneEmpty(tlsSettings);
    delete streamSettings.realitySettings;
  } else {
    delete streamSettings.tlsSettings;
  }

  if (state.security === "reality") {
    const realitySettings = cloneObject(objectValue(streamSettings.realitySettings));
    applyStringField(realitySettings, "serverName", state.realityServerName);
    applyStringField(realitySettings, "publicKey", state.realityPublicKey);
    applyStringField(realitySettings, "fingerprint", state.realityFingerprint);
    applyStringField(realitySettings, "spiderX", state.realitySpiderX);
    if (state.realityShortId.trim()) {
      realitySettings.shortId = state.realityShortId.trim();
      realitySettings.shortIds = [state.realityShortId.trim()];
    } else {
      delete realitySettings.shortId;
      delete realitySettings.shortIds;
    }
    streamSettings.realitySettings = pruneEmpty(realitySettings);
    delete streamSettings.tlsSettings;
  } else {
    delete streamSettings.realitySettings;
  }

  settings = pruneEmpty(settings);
  streamSettings = pruneEmpty(streamSettings);

  return {
    tag: state.tag.trim(),
    protocol: state.protocol,
    enabled: state.enabled,
    settings: stringifySlice(settings),
    streamSettings: stringifySlice(streamSettings)
  };
}

export function replaceOutboundSlice(state: OutboundEditorState, key: OutboundSliceKey, text: string): OutboundEditorState {
  const parsed = parseJsonSlice(text);
  const next = {
    ...state,
    [key]: {
      text,
      value: parsed.value,
      error: parsed.error
    }
  } as OutboundEditorState;
  return parsed.error ? next : syncStructuredFields(next);
}

export function syncOutboundAfterStructuredChange(state: OutboundEditorState): OutboundEditorState {
  const synced = buildOutboundInput(state);
  return syncStructuredFields({
    ...state,
    settings: nextSliceState(state.settings, synced.settings ?? ""),
    streamSettings: nextSliceState(state.streamSettings, synced.streamSettings ?? "")
  });
}

export function outboundSummary(outbound: Outbound): { network: string; security: string; detail: string } {
  const settings = parseJsonSlice(outbound.settings).value;
  const streamSettings = parseJsonSlice(outbound.streamSettings).value;
  const network = stringValue(streamSettings.network) ?? "tcp";
  const security = stringValue(streamSettings.security) ?? "none";
  const address = stringValue(settings.address);
  const port = numberString(settings.port);
  const server = stringValue(settings.server);
  const detail =
    outbound.protocol === "freedom"
      ? "direct"
      : server || (address && port ? `${address}:${port}` : address || "");
  return { network, security, detail };
}

export function validateOutboundState(state: OutboundEditorState): OutboundValidationIssue[] {
  const issues: OutboundValidationIssue[] = [];
  if (!state.tag.trim()) {
    issues.push({ field: "tag", message: "Tag is required." });
  }
  if (!state.enabled) return issues;

  if (state.protocol === "vless" || state.protocol === "vmess") {
    validateAddressPort(issues, state, state.protocol.toUpperCase());
    if (!state.userId.trim()) {
      issues.push({ field: "userId", message: `${state.protocol.toUpperCase()} requires a user ID.` });
    } else if (!isUuid(state.userId.trim())) {
      issues.push({ field: "userId", message: `${state.protocol.toUpperCase()} user ID must be a valid UUID.` });
    }
  }

  if (state.protocol === "trojan") {
    validateAddressPort(issues, state, "Trojan");
    if (!state.password.trim()) {
      issues.push({ field: "password", message: "Trojan outbound requires a password." });
    }
  }

  if (state.protocol === "shadowsocks") {
    validateAddressPort(issues, state, "Shadowsocks");
    if (!state.password.trim()) {
      issues.push({ field: "password", message: "Shadowsocks outbound requires a password." });
    }
  }

  if (state.protocol === "hysteria2" || state.protocol === "tuic") {
    const label = state.protocol === "tuic" ? "TUIC v5" : "Hysteria2";
    if (!state.server.trim()) {
      issues.push({ field: "server", message: `${label} outbound requires a server address.` });
    } else if (!isSocketAddr(state.server.trim())) {
      issues.push({ field: "server", message: `${label} server must look like 127.0.0.1:443 or [::1]:443.` });
    }
  }

  if (state.protocol === "tuic") {
    if (!state.userId.trim()) {
      issues.push({ field: "userId", message: "TUIC v5 requires a UUID." });
    } else if (!isUuid(state.userId.trim())) {
      issues.push({ field: "userId", message: "TUIC v5 UUID must be valid." });
    }
    if (!state.password.trim()) {
      issues.push({ field: "password", message: "TUIC v5 requires a password." });
    }
  }

  return issues;
}

function syncStructuredFields(state: OutboundEditorState): OutboundEditorState {
  const next = createOutboundEditorState({
    id: 0,
    tag: state.tag,
    protocol: state.protocol,
    enabled: state.enabled,
    settings: state.settings.text,
    streamSettings: state.streamSettings.text,
    createdAt: "",
    updatedAt: ""
  });
  return {
    ...state,
    network: next.network,
    security: next.security,
    address: next.address,
    port: next.port,
    userId: next.userId,
    password: next.password,
    method: next.method,
    server: next.server,
    hysteria2Auth: next.hysteria2Auth,
    hysteria2ServerName: next.hysteria2ServerName,
    hysteria2SkipCertVerify: next.hysteria2SkipCertVerify,
    hysteria2EndpointShards: next.hysteria2EndpointShards,
    hysteria2CongestionMode: next.hysteria2CongestionMode,
    hysteria2MinAckRate: next.hysteria2MinAckRate,
    hysteria2MaxQueueDelayMs: next.hysteria2MaxQueueDelayMs,
    hysteria2PacingGain: next.hysteria2PacingGain,
    hysteria2LossCompensation: next.hysteria2LossCompensation,
    hysteria2QuicReusePort: next.hysteria2QuicReusePort,
    hysteria2QuicEndpoints: next.hysteria2QuicEndpoints,
    hysteria2QuicRecvBufferBytes: next.hysteria2QuicRecvBufferBytes,
    hysteria2QuicSendBufferBytes: next.hysteria2QuicSendBufferBytes,
    hysteria2DatagramEnabled: next.hysteria2DatagramEnabled,
    hysteria2DatagramUdpOverDatagram: next.hysteria2DatagramUdpOverDatagram,
    hysteria2DatagramPolicy: next.hysteria2DatagramPolicy,
    hysteria2FecMode: next.hysteria2FecMode,
    hysteria2FecMaxOverheadPercent: next.hysteria2FecMaxOverheadPercent,
    wsPath: next.wsPath,
    wsHost: next.wsHost,
    grpcServiceName: next.grpcServiceName,
    httpupgradePath: next.httpupgradePath,
    httpupgradeHost: next.httpupgradeHost,
    splitHttpPath: next.splitHttpPath,
    kcpHeader: next.kcpHeader,
    kcpMtu: next.kcpMtu,
    kcpTti: next.kcpTti,
    kcpUplinkCapacity: next.kcpUplinkCapacity,
    kcpDownlinkCapacity: next.kcpDownlinkCapacity,
    kcpCongestion: next.kcpCongestion,
    kcpReadBufferSize: next.kcpReadBufferSize,
    kcpWriteBufferSize: next.kcpWriteBufferSize,
    tlsServerName: next.tlsServerName,
    tlsAlpn: next.tlsAlpn,
    tlsCertificateFile: next.tlsCertificateFile,
    tlsKeyFile: next.tlsKeyFile,
    realityServerName: next.realityServerName,
    realityPublicKey: next.realityPublicKey,
    realityShortId: next.realityShortId,
    realityFingerprint: next.realityFingerprint,
    realitySpiderX: next.realitySpiderX
  };
}

function createSliceState(raw?: string): SliceState {
  const parsed = parseJsonSlice(raw ?? "");
  return { text: raw ?? "", value: parsed.value, error: parsed.error };
}

function nextSliceState(current: SliceState, raw: string): SliceState {
  const parsed = parseJsonSlice(raw);
  return {
    text: current.error ? current.text : raw,
    value: parsed.value,
    error: current.error
  };
}

function parseJsonSlice(raw: string): { value: JsonObject; error: string } {
  const trimmed = raw.trim();
  if (!trimmed) return { value: {}, error: "" };
  try {
    const parsed = JSON.parse(trimmed);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      return { value: {}, error: "Expected a JSON object." };
    }
    return { value: parsed as JsonObject, error: "" };
  } catch (error) {
    return { value: {}, error: error instanceof Error ? error.message : "Invalid JSON." };
  }
}

function stringifySlice(value: JsonObject): string {
  return Object.keys(value).length === 0 ? "" : JSON.stringify(value, null, 2);
}

function pruneEmpty<T extends JsonObject>(value: T): T {
  const next = cloneObject(value);
  for (const [key, raw] of Object.entries(next)) {
    if (raw === undefined || raw === null || raw === "") {
      delete next[key];
      continue;
    }
    if (Array.isArray(raw)) {
      if (raw.length === 0) delete next[key];
      continue;
    }
    if (typeof raw === "object") {
      const child = pruneEmpty(raw as JsonObject);
      if (Object.keys(child).length === 0) delete next[key];
      else next[key] = child;
    }
  }
  return next as T;
}

function removeTransientNetworkKeys(streamSettings: JsonObject): JsonObject {
  const next = cloneObject(streamSettings);
  for (const key of ["wsSettings", "grpcSettings", "httpupgradeSettings", "splithttpSettings"]) {
    if (key === "splithttpSettings" || key === "httpupgradeSettings" || key === "grpcSettings" || key === "wsSettings") {
      // keep existing transient network cleanup grouped here
    }
    if (!objectValue(next[key])) delete next[key];
  }
  if (!objectValue(next.kcpSettings)) delete next.kcpSettings;
  return next;
}

function cloneObject(value?: JsonObject): JsonObject {
  return value ? JSON.parse(JSON.stringify(value)) : {};
}

function cloneUsers(value: unknown): JsonObject[] {
  return Array.isArray(value) ? value.filter((item): item is JsonObject => !!objectValue(item)).map((item) => cloneObject(item)) : [];
}

function firstUser(value: JsonObject): JsonObject | undefined {
  const users = Array.isArray(value.users) ? value.users : [];
  return objectValue(users[0]);
}

function objectValue(value: unknown): JsonObject | undefined {
  return value && typeof value === "object" && !Array.isArray(value) ? (value as JsonObject) : undefined;
}

function stringValue(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}

function boolValue(value: unknown): boolean | undefined {
  return typeof value === "boolean" ? value : undefined;
}

function numberString(value: unknown): string {
  return typeof value === "number" && Number.isFinite(value) ? String(value) : "";
}

function stringOrNumberString(value: unknown): string {
  if (typeof value === "string") return value;
  return numberString(value);
}

function arrayOfStrings(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : [];
}

function applyStringField(target: JsonObject, key: string, value: string) {
  const trimmed = value.trim();
  if (trimmed) target[key] = trimmed;
  else delete target[key];
}

function applyStringArrayField(target: JsonObject, key: string, value: string) {
  const list = value
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);
  if (list.length > 0) target[key] = list;
  else delete target[key];
}

function applyNumberField(target: JsonObject, key: string, value: string) {
  const trimmed = value.trim();
  if (!trimmed) {
    delete target[key];
    return;
  }
  const parsed = Number(trimmed);
  if (Number.isFinite(parsed)) target[key] = parsed;
}

function applyDefaultedNumberField(target: JsonObject, key: string, value: string, defaultValue: string) {
  if (value.trim() === defaultValue) delete target[key];
  else applyNumberField(target, key, value);
}

function applyNestedStringField(target: JsonObject, objectKey: string, key: string, value: string, defaultValue = "") {
  const child = cloneObject(objectValue(target[objectKey]));
  if (value.trim() === defaultValue) delete child[key];
  else applyStringField(child, key, value);
  setNestedObject(target, objectKey, child);
}

function applyNestedNumberField(target: JsonObject, objectKey: string, key: string, value: string, defaultValue = "") {
  const child = cloneObject(objectValue(target[objectKey]));
  if (value.trim() === defaultValue) delete child[key];
  else applyNumberField(child, key, value);
  setNestedObject(target, objectKey, child);
}

function applyNestedStringOrNumberField(target: JsonObject, objectKey: string, key: string, value: string, defaultValue = "") {
  const child = cloneObject(objectValue(target[objectKey]));
  const trimmed = value.trim();
  if (!trimmed || trimmed === defaultValue) {
    delete child[key];
  } else if (/^\d+$/.test(trimmed)) {
    child[key] = Number(trimmed);
  } else {
    child[key] = trimmed;
  }
  setNestedObject(target, objectKey, child);
}

function applyNestedBoolField(target: JsonObject, objectKey: string, key: string, value: boolean, defaultValue: boolean) {
  const child = cloneObject(objectValue(target[objectKey]));
  if (value === defaultValue) {
    delete child[key];
  } else {
    child[key] = value;
  }
  setNestedObject(target, objectKey, child);
}

function setNestedObject(target: JsonObject, key: string, value: JsonObject) {
  const pruned = pruneEmpty(value);
  if (Object.keys(pruned).length > 0) target[key] = pruned;
  else delete target[key];
}

function validateAddressPort(issues: OutboundValidationIssue[], state: OutboundEditorState, label: string) {
  if (!state.address.trim()) {
    issues.push({ field: "address", message: `${label} outbound requires an address.` });
  } else if (!isEndpointAddress(state.address.trim())) {
    issues.push({ field: "address", message: `${label} address must be an IPv4 address or bracketed IPv6 host.` });
  }

  const port = Number(state.port.trim());
  if (!state.port.trim()) {
    issues.push({ field: "port", message: `${label} outbound requires a port.` });
  } else if (!Number.isInteger(port) || port < 1 || port > 65535) {
    issues.push({ field: "port", message: `${label} port must be between 1 and 65535.` });
  }
}

function isUuid(value: string): boolean {
  return /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(value);
}

function isSocketAddr(value: string): boolean {
  const ipv4 = value.match(/^(\d{1,3}(?:\.\d{1,3}){3}):(\d{1,5})$/);
  if (ipv4) return isValidIpv4(ipv4[1]) && isValidPort(ipv4[2]);
  const ipv6 = value.match(/^\[([0-9a-fA-F:]+)\]:(\d{1,5})$/);
  if (ipv6) return ipv6[1].includes(":") && isValidPort(ipv6[2]);
  return false;
}

function isEndpointAddress(value: string): boolean {
  return isValidIpv4(value) || /^\[[0-9a-fA-F:]+\]$/.test(value);
}

function isValidIpv4(value: string): boolean {
  const parts = value.split(".");
  return (
    parts.length === 4 &&
    parts.every((part) => /^\d{1,3}$/.test(part) && Number(part) >= 0 && Number(part) <= 255)
  );
}

function isValidPort(value: string): boolean {
  const port = Number(value);
  return Number.isInteger(port) && port >= 1 && port <= 65535;
}
