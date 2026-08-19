import type {
  ApplyResult,
  CapabilityMap,
  CoreSettings,
  Inbound,
  InboundInput,
  LoginResponse,
  ManagedUser,
  Outbound,
  OutboundInput,
  RealityClientValues,
  RealityGeneratedValues,
  RevisionSummary,
  RoutingDns,
  ServiceStatus,
  Settings,
  Status,
  TlsSelfSignedInput,
  TlsSelfSignedResult,
  TlsServerValues,
  TrafficSnapshot,
  UserInput
} from "./types";

const SESSION_MARKER_KEY = "black-ui-session";
let desiredRevision: number | null = null;

export function getToken(): string {
  return sessionStorage.getItem(SESSION_MARKER_KEY) ?? "";
}

export function setToken(): void {
  sessionStorage.setItem(SESSION_MARKER_KEY, "cookie");
}

export function clearToken(): void {
  sessionStorage.removeItem(SESSION_MARKER_KEY);
}

async function request<T>(path: string, options: RequestInit = {}): Promise<T> {
  const headers = new Headers(options.headers);
  headers.set("X-Black-UI-Request", "fetch");
  if (options.body && !headers.has("Content-Type")) headers.set("Content-Type", "application/json");
  if (options.method && options.method !== "GET" && desiredRevision !== null) {
    headers.set("If-Match", `"${desiredRevision}"`);
  }

  const res = await fetch(path, { ...options, credentials: "same-origin", headers });
  const contentType = res.headers.get("content-type") ?? "";
  const payload = contentType.includes("application/json") ? await res.json() : await res.text();
  if (!res.ok) {
    const message = typeof payload === "object" && payload && "error" in payload ? String(payload.error) : String(payload);
    throw new Error(message || `${res.status} ${res.statusText}`);
  }
  if (typeof payload === "object" && payload) {
    if ("desiredRevision" in payload && typeof payload.desiredRevision === "number") {
      desiredRevision = payload.desiredRevision;
    } else if ("revision" in payload && "parentRevision" in payload && typeof payload.revision === "number") {
      desiredRevision = payload.revision;
    }
  }
  return payload as T;
}

const body = (value: unknown): RequestInit => ({
  method: "POST",
  body: JSON.stringify(value)
});

export const api = {
  status: () => request<Status>("/api/status"),
  capabilities: () => request<CapabilityMap>("/api/capabilities"),
  me: () => request<{ username: string }>("/api/auth/me"),
  authStatus: async () => {
    try {
      return await request<{ setupRequired: boolean }>("/api/auth/status");
    } catch (error) {
      // v0.2.0 did not have the minimal bootstrap endpoint. Its status route
      // is public only before the first admin exists, so an authentication
      // response means a login form—not an indefinitely disabled screen.
      if (!(error instanceof Error) || !error.message.includes("404")) throw error;
      try {
        const legacy = await request<Status>("/api/status");
        return { setupRequired: legacy.setupRequired };
      } catch (legacyError) {
        if (legacyError instanceof Error && legacyError.message.includes("authentication")) {
          return { setupRequired: false };
        }
        throw legacyError;
      }
    }
  },
  setup: (username: string, password: string) =>
    request<LoginResponse>("/api/auth/setup", body({ username, password })),
  login: (username: string, password: string) =>
    request<LoginResponse>("/api/auth/login", body({ username, password })),
  logout: () => request<{ ok: boolean }>("/api/auth/logout", { method: "POST" }),
  settings: () => request<Settings>("/api/settings"),
  updateSettings: (settings: Settings) =>
    request<Settings>("/api/settings", { method: "PUT", body: JSON.stringify(settings) }),
  traffic: () => request<TrafficSnapshot>("/api/runtime/traffic"),
  realityClientValues: () => request<RealityClientValues[]>("/api/reality/client-values"),
  realityGenerateValues: () => request<RealityGeneratedValues>("/api/reality/generate-values", { method: "POST" }),
  tlsServerValues: () => request<TlsServerValues[]>("/api/tls/server-values"),
  tlsGenerateSelfSigned: (input: TlsSelfSignedInput) =>
    request<TlsSelfSignedResult>("/api/tls/generate-self-signed", body(input)),
  revisions: () => request<RevisionSummary[]>("/api/runtime/revisions"),
  routingDns: () => request<RoutingDns>("/api/routing-dns"),
  updateRoutingDns: (value: RoutingDns) => request<ApplyResult>("/api/routing-dns", { method: "PUT", body: JSON.stringify(value) }),
  coreSettings: () => request<CoreSettings>("/api/core-settings"),
  updateCoreSettings: (value: CoreSettings) => request<ApplyResult>("/api/core-settings", { method: "PUT", body: JSON.stringify(value) }),
  rollback: (revision: number) => request<ApplyResult>("/api/runtime/rollback", body({ revision })),
  inbounds: () => request<Inbound[]>("/api/inbounds"),
  createInbound: (input: InboundInput) => request<ApplyResult>("/api/inbounds", body(input)),
  updateInbound: (id: number, input: InboundInput) =>
    request<ApplyResult>(`/api/inbounds/${id}`, { method: "PUT", body: JSON.stringify(input) }),
  deleteInbound: (id: number) => request<ApplyResult>(`/api/inbounds/${id}`, { method: "DELETE" }),
  outbounds: () => request<Outbound[]>("/api/outbounds"),
  createOutbound: (input: OutboundInput) => request<ApplyResult>("/api/outbounds", body(input)),
  updateOutbound: (id: number, input: OutboundInput) =>
    request<ApplyResult>(`/api/outbounds/${id}`, { method: "PUT", body: JSON.stringify(input) }),
  deleteOutbound: (id: number) => request<ApplyResult>(`/api/outbounds/${id}`, { method: "DELETE" }),
  users: () => request<ManagedUser[]>("/api/users"),
  createUser: (input: UserInput) => request<ApplyResult>("/api/users", body(input)),
  updateUser: (id: number, input: UserInput) =>
    request<ApplyResult>(`/api/users/${id}`, { method: "PUT", body: JSON.stringify(input) }),
  deleteUser: (id: number) => request<ApplyResult>(`/api/users/${id}`, { method: "DELETE" }),
  enableUser: (id: number) => request<ApplyResult>(`/api/users/${id}/enable`, { method: "POST" }),
  disableUser: (id: number) => request<ApplyResult>(`/api/users/${id}/disable`, { method: "POST" }),
  resetUsage: (id: number) => request<ManagedUser>(`/api/users/${id}/reset-usage`, { method: "POST" }),
  rotateUuid: (id: number) => request<ApplyResult>(`/api/users/${id}/rotate-uuid`, { method: "POST" }),
  rotateSubToken: (id: number) => request<ManagedUser>(`/api/users/${id}/rotate-sub-token`, { method: "POST" }),
  bulkUsers: (payload: {
    userIds: number[];
    action: string;
    trafficLimitBytes?: number | null;
    expiryAt?: string | null;
  }) => request<ApplyResult>("/api/users/bulk", body(payload)),
  uuid: () => request<{ uuid: string }>("/api/uuid", { method: "POST" }),
  serviceStatus: () => request<ServiceStatus>("/api/service/status"),
  serviceStartBlackwire: () => request<ServiceStatus>("/api/service/start-blackwire", { method: "POST" }),
  serviceStopBlackwire: () => request<ServiceStatus>("/api/service/stop-blackwire", { method: "POST" }),
  serviceLogs: () => request<string[]>("/api/service/logs")
};
