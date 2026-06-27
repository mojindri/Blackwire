import type { Inbound, InboundInput } from "./types";

export type JsonObject = Record<string, unknown>;
export type SliceKey = "settings" | "streamSettings" | "sniffing" | "limits";

export interface SliceState {
  text: string;
  value: JsonObject;
  error: string;
}

export interface InboundEditorState {
  tag: string;
  listen: string;
  port: number;
  protocol: string;
  enabled: boolean;
  network: string;
  security: string;
  decryption: string;
  shadowsocksMethod: string;
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
  realityPrivateKey: string;
  realityPublicKey: string;
  realityShortId: string;
  realityDest: string;
  realityFingerprint: string;
  realitySpiderX: string;
  sniffingEnabled: boolean;
  sniffingDestOverride: string[];
  sniffingMetadataOnly: boolean;
  sniffingRouteOnly: boolean;
  maxConnections: string;
  maxHandshakeSeconds: string;
  settings: SliceState;
  streamSettings: SliceState;
  sniffing: SliceState;
  limits: SliceState;
}

export interface InboundValidationIssue {
  field: string;
  message: string;
}

export interface InboundCompatibilityNotice {
  tone: "info" | "warning";
  message: string;
}

const DEFAULT_PROTOCOL = "vless";
const DEFAULT_NETWORK = "tcp";
const DEFAULT_SECURITY = "none";

export function createInboundEditorState(inbound?: Inbound | null): InboundEditorState {
  const settings = createSliceState(inbound?.settings);
  const streamSettings = createSliceState(inbound?.streamSettings);
  const sniffing = createSliceState(inbound?.sniffing);
  const limits = createSliceState(inbound?.limits);

  const network =
    stringValue(streamSettings.value.network) ??
    transportToNetwork(inbound?.transport) ??
    DEFAULT_NETWORK;
  const security =
    stringValue(streamSettings.value.security) ??
    (inbound?.transport === "reality" ? "reality" : DEFAULT_SECURITY);

  return {
    tag: inbound?.tag ?? "vless-main",
    listen: inbound?.listen ?? "0.0.0.0",
    port: inbound?.port ?? 443,
    protocol: inbound?.protocol ?? DEFAULT_PROTOCOL,
    enabled: inbound?.enabled ?? true,
    network,
    security,
    decryption: stringValue(settings.value.decryption) ?? "none",
    shadowsocksMethod: stringValue(settings.value.method) ?? "",
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
    realityServerName:
      stringValue(objectValue(streamSettings.value.realitySettings)?.serverName) ??
      arrayOfStrings(objectValue(streamSettings.value.realitySettings)?.serverNames)[0] ??
      "",
    realityPrivateKey: stringValue(objectValue(streamSettings.value.realitySettings)?.privateKey) ?? "",
    realityPublicKey: stringValue(objectValue(streamSettings.value.realitySettings)?.publicKey) ?? "",
    realityShortId:
      stringValue(objectValue(streamSettings.value.realitySettings)?.shortId) ??
      arrayOfStrings(objectValue(streamSettings.value.realitySettings)?.shortIds)[0] ??
      "",
    realityDest: stringValue(objectValue(streamSettings.value.realitySettings)?.dest) ?? "",
    realityFingerprint: stringValue(objectValue(streamSettings.value.realitySettings)?.fingerprint) ?? "chrome",
    realitySpiderX: stringValue(objectValue(streamSettings.value.realitySettings)?.spiderX) ?? "/",
    sniffingEnabled: boolValue(sniffing.value.enabled) ?? false,
    sniffingDestOverride: arrayOfStrings(sniffing.value.destOverride),
    sniffingMetadataOnly: boolValue(sniffing.value.metadataOnly) ?? false,
    sniffingRouteOnly: boolValue(sniffing.value.routeOnly) ?? false,
    maxConnections: numberString(limits.value.maxConnections),
    maxHandshakeSeconds: numberString(limits.value.maxHandshakeSeconds),
    settings,
    streamSettings,
    sniffing,
    limits
  };
}

export function buildInboundInput(state: InboundEditorState): InboundInput {
  let settings = cloneObject(state.settings.value);
  let streamSettings = cloneObject(state.streamSettings.value);
  let sniffing = cloneObject(state.sniffing.value);
  let limits = cloneObject(state.limits.value);

  settings = cleanManagedClients(settings);
  streamSettings = removeTransientNetworkKeys(streamSettings);

  if (state.protocol === "vless" && state.decryption.trim()) {
    settings.decryption = state.decryption.trim();
  } else {
    delete settings.decryption;
  }

  if (state.protocol === "shadowsocks" && state.shadowsocksMethod.trim()) {
    settings.method = state.shadowsocksMethod.trim();
  } else if (state.protocol !== "shadowsocks") {
    delete settings.method;
  }

  streamSettings.network = state.network;
  streamSettings.security = state.security;

  if (state.network === "ws") {
    const wsSettings = cloneObject(objectValue(streamSettings.wsSettings));
    wsSettings.path = state.wsPath.trim() || "/";
    const headers = cloneObject(objectValue(wsSettings.headers));
    if (state.wsHost.trim()) {
      headers.Host = state.wsHost.trim();
    } else {
      delete headers.Host;
    }
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
    if (state.httpupgradeHost.trim()) {
      httpupgradeSettings.host = state.httpupgradeHost.trim();
    } else {
      delete httpupgradeSettings.host;
    }
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
    const realityServerName = state.realityServerName.trim();
    applyStringField(realitySettings, "serverName", realityServerName);
    if (realityServerName) {
      realitySettings.serverNames = [realityServerName];
      realitySettings.maxTimeDiffSeconds = 60;
      delete realitySettings.maxTimeDiff;
    } else {
      delete realitySettings.serverNames;
      delete realitySettings.maxTimeDiffSeconds;
      delete realitySettings.maxTimeDiff;
    }
    applyStringField(realitySettings, "privateKey", state.realityPrivateKey);
    applyStringField(realitySettings, "publicKey", state.realityPublicKey);
    applyStringField(realitySettings, "dest", state.realityDest);
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

  if (!state.sniffingEnabled && state.sniffingDestOverride.length === 0 && !state.sniffingMetadataOnly && !state.sniffingRouteOnly) {
    sniffing = {};
  } else {
    sniffing.enabled = state.sniffingEnabled;
    sniffing.destOverride = state.sniffingDestOverride;
    if (state.sniffingMetadataOnly) {
      sniffing.metadataOnly = true;
    } else {
      delete sniffing.metadataOnly;
    }
    if (state.sniffingRouteOnly) {
      sniffing.routeOnly = true;
    } else {
      delete sniffing.routeOnly;
    }
  }

  if (state.maxConnections.trim()) {
    limits.maxConnections = Number(state.maxConnections);
  } else {
    delete limits.maxConnections;
  }
  if (state.maxHandshakeSeconds.trim()) {
    limits.maxHandshakeSeconds = Number(state.maxHandshakeSeconds);
  } else {
    delete limits.maxHandshakeSeconds;
  }

  streamSettings = pruneEmpty(streamSettings);
  settings = pruneEmpty(settings);
  sniffing = pruneEmpty(sniffing);
  limits = pruneEmpty(limits);

  return {
    tag: state.tag.trim(),
    listen: state.listen.trim(),
    port: state.port,
    protocol: state.protocol,
    enabled: state.enabled,
    transport: state.security === "reality" ? "reality" : state.network,
    settings: stringifySlice(settings),
    streamSettings: stringifySlice(streamSettings),
    sniffing: stringifySlice(sniffing),
    limits: stringifySlice(limits)
  };
}

export function replaceSlice(state: InboundEditorState, key: SliceKey, text: string): InboundEditorState {
  const parsed = parseJsonSlice(text);
  const next = {
    ...state,
    [key]: {
      text,
      value: parsed.value,
      error: parsed.error
    }
  } as InboundEditorState;
  return parsed.error ? next : syncStructuredFields(next);
}

export function syncAfterStructuredChange(state: InboundEditorState): InboundEditorState {
  const synced = buildInboundInput(state);
  return syncStructuredFields({
    ...state,
    settings: nextSliceState(state.settings, synced.settings ?? ""),
    streamSettings: nextSliceState(state.streamSettings, synced.streamSettings ?? ""),
    sniffing: nextSliceState(state.sniffing, synced.sniffing ?? ""),
    limits: nextSliceState(state.limits, synced.limits ?? "")
  });
}

export function inboundSummary(inbound: Inbound): { network: string; security: string; detail: string } {
  const streamSettings = parseJsonSlice(inbound.streamSettings).value;
  const network = stringValue(streamSettings.network) ?? transportToNetwork(inbound.transport) ?? inbound.transport;
  const security = stringValue(streamSettings.security) ?? (inbound.transport === "reality" ? "reality" : "none");
  const wsPath = stringValue(objectValue(streamSettings.wsSettings)?.path);
  const grpcServiceName = stringValue(objectValue(streamSettings.grpcSettings)?.serviceName);
  const detail = wsPath ? wsPath : grpcServiceName ? grpcServiceName : "";
  return { network, security, detail };
}

export function inboundCompatibilityNotice(
  input: Pick<InboundEditorState, "protocol" | "network" | "security"> | Inbound
): InboundCompatibilityNotice | null {
  const protocol = input.protocol;
  const network =
    "network" in input
      ? input.network
      : inboundSummary(input).network;
  const security =
    "security" in input
      ? input.security
      : inboundSummary(input).security;

  if (protocol === "tuic") {
    return {
      tone: "info",
      message:
        "TUIC v5 is supported for QUIC TCP proxying and native UDP relay. Throughput and UDP behavior still depend on the client, firewall, NAT, and network path."
    };
  }
  if (network === "quic") {
    const prefix = protocol === "vmess" ? "VMess over QUIC" : "QUIC transport";
    return {
      tone: "warning",
      message: `${prefix} remains available, but external-client support varies and UDP route issues can break global/TUN traffic. Prefer TCP, WebSocket, gRPC, or SplitHTTP for the main profile.`
    };
  }
  if (protocol === "hysteria2") {
    return {
      tone: "info",
      message:
        "Hysteria2 is available, but throughput depends heavily on UDP path, CPU, and bandwidth tuning. Validate with a dedicated speed test before switching system-wide."
    };
  }
  if (security === "reality" && network !== "tcp") {
    return {
      tone: "warning",
      message:
        "REALITY is best on TCP-compatible transport paths, but this editor allows it on any transport for manual testing."
    };
  }
  return null;
}

export function validateInboundState(state: InboundEditorState): InboundValidationIssue[] {
  const issues: InboundValidationIssue[] = [];
  if (!state.tag.trim()) {
    issues.push({ field: "tag", message: "Tag is required." });
  }
  if (!state.listen.trim()) {
    issues.push({ field: "listen", message: "Listen host is required." });
  } else if (!isIpAddress(state.listen.trim())) {
    issues.push({ field: "listen", message: "Listen host should be an IPv4 or IPv6 address." });
  } else if (state.enabled && isPublicClientInbound(state) && isLocalListenHost(state.listen.trim())) {
    issues.push({
      field: "listen",
      message: `${state.protocol} over ${state.network} must use a public bind address such as 0.0.0.0 or ::. 127.0.0.1 is local-only.`
    });
  }
  if (!Number.isInteger(state.port) || state.port < 1 || state.port > 65535) {
    issues.push({ field: "port", message: "Port must be between 1 and 65535." });
  }
  if (state.security === "tls") {
    if (!state.tlsServerName.trim()) {
      issues.push({ field: "tlsServerName", message: "TLS server name is required for generated public client links." });
    }
    if (state.tlsCertificateFile.trim() && !state.tlsKeyFile.trim()) {
      issues.push({ field: "tlsKeyFile", message: "TLS key file is required when a certificate file is set." });
    }
    if (state.tlsKeyFile.trim() && !state.tlsCertificateFile.trim()) {
      issues.push({ field: "tlsCertificateFile", message: "TLS certificate file is required when a key file is set." });
    }
  }
  if (state.security === "reality") {
    if (!state.realityServerName.trim()) {
      issues.push({ field: "realityServerName", message: "REALITY server name is required." });
    }
    if (!state.realityPrivateKey.trim()) {
      issues.push({ field: "realityPrivateKey", message: "REALITY private key is required for the server listener." });
    } else if (!isRealityPrivateKey(state.realityPrivateKey.trim())) {
      issues.push({ field: "realityPrivateKey", message: "REALITY private key must be a 32-byte hex X25519 private key." });
    }
    if (!state.realityPublicKey.trim()) {
      issues.push({ field: "realityPublicKey", message: "REALITY public key is required and must come from the server key pair." });
    } else if (!isRealityPublicKey(state.realityPublicKey.trim())) {
      issues.push({ field: "realityPublicKey", message: "REALITY public key must be a 32-byte hex key or base64url X25519 public key from the server." });
    }
    if (!state.realityShortId.trim()) {
      issues.push({ field: "realityShortId", message: "REALITY short ID is required and must match a server shortIds entry." });
    } else if (!isRealityShortId(state.realityShortId.trim())) {
      issues.push({ field: "realityShortId", message: "REALITY short ID must be even-length hex, up to 16 characters." });
    }
    if (!state.realityDest.trim()) {
      issues.push({ field: "realityDest", message: "REALITY fallback destination is required." });
    } else if (!isRealityDest(state.realityDest.trim())) {
      issues.push({ field: "realityDest", message: "REALITY fallback destination must be an IP socket address like 93.184.216.34:443." });
    }
  }
  return issues;
}

function syncStructuredFields(state: InboundEditorState): InboundEditorState {
  const next = createInboundEditorState({
    id: 0,
    tag: state.tag,
    listen: state.listen,
    port: state.port,
    protocol: state.protocol,
    enabled: state.enabled,
    transport: state.security === "reality" ? "reality" : state.network,
    settings: state.settings.text,
    streamSettings: state.streamSettings.text,
    sniffing: state.sniffing.text,
    limits: state.limits.text,
    createdAt: "",
    updatedAt: ""
  });
  return {
    ...state,
    decryption: next.decryption,
    shadowsocksMethod: next.shadowsocksMethod,
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
    realityPrivateKey: next.realityPrivateKey,
    realityPublicKey: next.realityPublicKey,
    realityShortId: next.realityShortId,
    realityDest: next.realityDest,
    realityFingerprint: next.realityFingerprint,
    realitySpiderX: next.realitySpiderX,
    sniffingEnabled: next.sniffingEnabled,
    sniffingDestOverride: next.sniffingDestOverride,
    sniffingMetadataOnly: next.sniffingMetadataOnly,
    sniffingRouteOnly: next.sniffingRouteOnly,
    maxConnections: next.maxConnections,
    maxHandshakeSeconds: next.maxHandshakeSeconds
  };
}

function createSliceState(raw?: string): SliceState {
  const parsed = parseJsonSlice(raw ?? "");
  return {
    text: raw ?? "",
    value: parsed.value,
    error: parsed.error
  };
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

function transportToNetwork(transport?: string): string | undefined {
  if (!transport) return undefined;
  if (transport === "reality") return "tcp";
  return transport;
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
      if (Object.keys(child).length === 0) {
        delete next[key];
      } else {
        next[key] = child;
      }
    }
  }
  return next as T;
}

function cleanManagedClients(settings: JsonObject): JsonObject {
  const next = cloneObject(settings);
  delete next.clients;
  return next;
}

function removeTransientNetworkKeys(streamSettings: JsonObject): JsonObject {
  const next = cloneObject(streamSettings);
  for (const key of [
    "wsSettings",
    "grpcSettings",
    "httpupgradeSettings",
    "splithttpSettings",
    "kcpSettings"
  ]) {
    if (!objectValue(next[key])) delete next[key];
  }
  return next;
}

function cloneObject(value?: JsonObject): JsonObject {
  return value ? JSON.parse(JSON.stringify(value)) : {};
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

function arrayOfStrings(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : [];
}

function numberString(value: unknown): string {
  return typeof value === "number" && Number.isFinite(value) ? String(value) : "";
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
  else delete target[key];
}

function isIpAddress(value: string): boolean {
  return isValidIpv4(value) || isLikelyIpv6(value);
}

function isLocalListenHost(value: string): boolean {
  const normalized = value.trim().replace(/^\[|\]$/g, "").toLowerCase();
  return normalized === "localhost" || normalized === "::1" || normalized.startsWith("127.");
}

function isPublicClientInbound(state: Pick<InboundEditorState, "protocol">): boolean {
  return ["vless", "vmess", "trojan", "shadowsocks", "hysteria2", "tuic"].includes(state.protocol);
}

function isValidIpv4(value: string): boolean {
  const parts = value.split(".");
  return (
    parts.length === 4 &&
    parts.every((part) => /^\d{1,3}$/.test(part) && Number(part) >= 0 && Number(part) <= 255)
  );
}

function isLikelyIpv6(value: string): boolean {
  return /^[0-9a-fA-F:]+$/.test(value) && value.includes(":");
}

function isRealityPublicKey(value: string): boolean {
  if (/^[0-9a-fA-F]{64}$/.test(value)) {
    return true;
  }
  if (/^[A-Za-z0-9_-]{43}$/.test(value)) {
    return true;
  }
  return /^[A-Za-z0-9+/]{43}=?$/.test(value);
}

function isRealityPrivateKey(value: string): boolean {
  return /^[0-9a-fA-F]{64}$/.test(value);
}

function isRealityShortId(value: string): boolean {
  return value.length > 0 && value.length <= 16 && value.length % 2 === 0 && /^[0-9a-fA-F]+$/.test(value);
}

function isRealityDest(value: string): boolean {
  if (/^\d{1,3}(\.\d{1,3}){3}:\d{1,5}$/.test(value)) {
    const [host, port] = value.split(":");
    const portNumber = Number(port);
    return isValidIpv4(host) && Number.isInteger(portNumber) && portNumber >= 1 && portNumber <= 65535;
  }
  const match = value.match(/^\[([0-9a-fA-F:]+)\]:(\d{1,5})$/);
  if (!match) return false;
  const portNumber = Number(match[2]);
  return isLikelyIpv6(match[1]) && Number.isInteger(portNumber) && portNumber >= 1 && portNumber <= 65535;
}
