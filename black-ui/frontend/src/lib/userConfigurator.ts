import type { Inbound, ManagedUser, UserInput } from "./types";

export type UserProtocol = "vless" | "vmess" | "trojan" | "shadowsocks" | "hysteria2" | "unknown";

export interface UserEditorState {
  inboundId: number;
  email: string;
  uuid: string;
  flow: string;
  password: string;
  auth: string;
  method: string;
  note: string;
  enabled: boolean;
  trafficLimitGB: string;
  expiryLocal: string;
  credentialText: string;
  credentialError: string;
}

export interface UserValidationIssue {
  field: string;
  message: string;
}

const EMPTY_CREDENTIAL = "{}";

export function createUserEditorState(user: ManagedUser | null, inbounds: Inbound[]): UserEditorState {
  const defaultInboundId = inbounds[0]?.id ?? 0;
  const inboundId = user?.inboundId ?? defaultInboundId;
  const credential = user?.credential ?? {};
  const credentialText = stringifyCredential(credential);

  return {
    inboundId,
    email: user?.email ?? "",
    uuid: user?.uuid ?? "",
    flow: user?.flow ?? "",
    password: stringValue(credential.password) ?? "",
    auth: stringValue(credential.auth) ?? "",
    method: stringValue(credential.method) ?? "",
    note: user?.note ?? "",
    enabled: user?.enabled ?? true,
    trafficLimitGB: bytesToGigabytesString(user?.trafficLimitBytes ?? null),
    expiryLocal: toInputDateTime(user?.expiryAt ?? null),
    credentialText,
    credentialError: ""
  };
}

export function replaceCredentialJson(state: UserEditorState, text: string): UserEditorState {
  const parsed = parseCredential(text);
  if (parsed.error) {
    return { ...state, credentialText: text, credentialError: parsed.error };
  }
  return {
    ...state,
    credentialText: text,
    credentialError: "",
    password: stringValue(parsed.value.password) ?? "",
    auth: stringValue(parsed.value.auth) ?? "",
    method: stringValue(parsed.value.method) ?? ""
  };
}

export function syncCredentialFromFields(state: UserEditorState, protocol: UserProtocol): UserEditorState {
  const parsed = parseCredential(state.credentialText);
  if (parsed.error) {
    return { ...state, credentialError: parsed.error };
  }

  const credential = cloneObject(parsed.value);

  delete credential.password;
  delete credential.auth;
  delete credential.method;

  if (protocol === "trojan" || protocol === "shadowsocks") {
    if (state.password.trim()) credential.password = state.password.trim();
  }
  if (protocol === "hysteria2") {
    if (state.auth.trim()) credential.auth = state.auth.trim();
  }
  if (protocol === "shadowsocks" && state.method.trim()) {
    credential.method = state.method.trim();
  }

  return {
    ...state,
    credentialText: stringifyCredential(credential),
    credentialError: ""
  };
}

export function buildUserInput(state: UserEditorState, protocol: UserProtocol): UserInput {
  const credential = parseCredential(state.credentialText).value;
  const next = cloneObject(credential);

  delete next.password;
  delete next.auth;
  delete next.method;

  if (protocol === "trojan" || protocol === "shadowsocks") {
    if (state.password.trim()) next.password = state.password.trim();
  }
  if (protocol === "hysteria2") {
    if (state.auth.trim()) next.auth = state.auth.trim();
  }
  if (protocol === "shadowsocks" && state.method.trim()) {
    next.method = state.method.trim();
  }

  return {
    inboundId: state.inboundId,
    email: state.email.trim(),
    uuid: state.uuid.trim(),
    flow: protocol === "vless" && state.flow.trim() ? state.flow.trim() : "",
    credential: next,
    note: state.note.trim(),
    enabled: state.enabled,
    trafficLimitBytes: gigabytesStringToBytes(state.trafficLimitGB),
    expiryAt: fromInputDateTime(state.expiryLocal)
  };
}

export function activeUserProtocol(inbounds: Inbound[], inboundId: number): UserProtocol {
  const protocol = inbounds.find((item) => item.id === inboundId)?.protocol;
  if (protocol === "vless" || protocol === "vmess" || protocol === "trojan" || protocol === "shadowsocks" || protocol === "hysteria2") {
    return protocol;
  }
  return "unknown";
}

export function validateUserState(state: UserEditorState, protocol: UserProtocol, inbounds: Inbound[]): UserValidationIssue[] {
  const issues: UserValidationIssue[] = [];
  if (!state.email.trim()) {
    issues.push({ field: "email", message: "Email is required." });
  }
  if (!inbounds.some((item) => item.id === state.inboundId)) {
    issues.push({ field: "inboundId", message: "Inbound is required." });
  }
  if (!state.uuid.trim()) {
    issues.push({ field: "uuid", message: "UUID is required." });
  } else if (!isUuid(state.uuid.trim())) {
    issues.push({ field: "uuid", message: "UUID must be valid." });
  }
  if (protocol === "trojan" || protocol === "shadowsocks") {
    if (!state.password.trim()) {
      issues.push({ field: "password", message: `${protocol === "trojan" ? "Trojan" : "Shadowsocks"} users require a password.` });
    }
  }
  if (protocol === "hysteria2" && !state.auth.trim()) {
    issues.push({ field: "auth", message: "Hysteria2 users require auth." });
  }
  if (state.trafficLimitGB.trim()) {
    const parsed = Number(state.trafficLimitGB.trim());
    if (!Number.isFinite(parsed) || parsed <= 0) {
      issues.push({ field: "trafficLimitGB", message: "Traffic limit must be a positive GB value." });
    }
  }
  if (state.credentialError) {
    issues.push({ field: "credential", message: "Advanced credential JSON is invalid." });
  }
  return issues;
}

export function protocolLabel(protocol: UserProtocol): string {
  if (protocol === "unknown") return "custom";
  return protocol;
}

function parseCredential(raw: string): { value: Record<string, unknown>; error: string } {
  const trimmed = raw.trim();
  if (!trimmed) return { value: {}, error: "" };
  try {
    const parsed = JSON.parse(trimmed);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      return { value: {}, error: "Expected a JSON object." };
    }
    return { value: parsed as Record<string, unknown>, error: "" };
  } catch (error) {
    return { value: {}, error: error instanceof Error ? error.message : "Invalid JSON." };
  }
}

function stringifyCredential(value: Record<string, unknown>): string {
  return Object.keys(value).length === 0 ? EMPTY_CREDENTIAL : JSON.stringify(value, null, 2);
}

function cloneObject(value?: Record<string, unknown>): Record<string, unknown> {
  return value ? JSON.parse(JSON.stringify(value)) : {};
}

function stringValue(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}

function bytesToGigabytesString(bytes: number | null): string {
  if (!bytes || bytes <= 0) return "";
  const value = bytes / (1024 * 1024 * 1024);
  return value % 1 === 0 ? String(value) : value.toFixed(2).replace(/\.?0+$/, "");
}

function gigabytesStringToBytes(value: string): number | null {
  const trimmed = value.trim();
  if (!trimmed) return null;
  const parsed = Number(trimmed);
  if (!Number.isFinite(parsed) || parsed <= 0) return null;
  return Math.round(parsed * 1024 * 1024 * 1024);
}

function isUuid(value: string): boolean {
  return /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(value);
}

function toInputDateTime(value: string | null): string {
  if (!value) return "";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "";
  const offset = date.getTimezoneOffset() * 60000;
  return new Date(date.getTime() - offset).toISOString().slice(0, 16);
}

function fromInputDateTime(value: string): string | null {
  if (!value) return null;
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? null : date.toISOString();
}
