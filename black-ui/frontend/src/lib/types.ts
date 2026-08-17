export type PageKey = "dashboard" | "users" | "inbounds" | "outbounds" | "sections" | "service" | "settings";

export interface Settings {
  firewallAutoOpen: boolean;
  publicBaseUrl: string;
  subscriptionHost: string;
  enforcementIntervalSeconds: number;
  adaptiveRoutingEnabled: boolean;
  adaptiveTuningMode: "off" | "recommend" | "auto" | string;
  adaptiveTuningIntervalSeconds: number;
  adaptiveTuningCooldownSeconds: number;
  adaptiveTuningMaxHysteria2Mbps: number;
  adaptiveTuningState: Record<string, unknown>;
}

export interface Status {
  setupRequired: boolean;
  databaseConnected: boolean;
  schemaVersion: number;
  desiredRevision: number;
  activeRevision: number | null;
  pendingMaintenanceRevision: number | null;
  activationState: "active" | "activating" | "pendingMaintenance" | "failed";
  lastActivationError: string | null;
  runtimeReachable: boolean;
  lastReconciliation: string;
  inbounds: number;
  outbounds: number;
  users: number;
  activeUsers: number;
}

export interface RealityClientValues {
  source: string;
  tag: string | null;
  address: string | null;
  port: number | null;
  uuid: string | null;
  privateKey: string | null;
  publicKey: string;
  shortId: string;
  serverName: string;
  dest: string | null;
}

export interface RealityGeneratedValues {
  privateKey: string;
  publicKey: string;
  shortId: string;
}

export interface TlsServerValues {
  source: string;
  tag: string | null;
  port: number | null;
  serverName: string | null;
  alpn: string[];
  certificateFile: string | null;
  keyFile: string | null;
  allowInsecure: boolean;
}

export interface TlsSelfSignedInput {
  serverName: string;
  days: number;
}

export interface TlsSelfSignedResult {
  serverName: string;
  certificateFile: string;
  keyFile: string;
  days: number;
}

export interface Inbound {
  id: number;
  tag: string;
  listen: string;
  port: number;
  protocol: string;
  enabled: boolean;
  transport: string;
  security: string;
}

export interface InboundInput {
  tag: string;
  listen: string;
  port: number;
  protocol: string;
  enabled: boolean;
  transport: string;
  security: string;
}

export interface Outbound {
  id: number;
  tag: string;
  protocol: string;
  enabled: boolean;
  address: string | null;
  port: number | null;
  transport: string;
  security: string;
}

export interface OutboundInput {
  tag: string;
  protocol: string;
  enabled: boolean;
  address: string | null;
  port: number | null;
  transport: string;
  security: string;
}

export interface ManagedUser {
  id: number;
  inboundId: number;
  email: string;
  uuid: string;
  flow: string;
  credentialKind: string;
  method: string | null;
  note: string;
  enabled: boolean;
  trafficLimitBytes: number | null;
  expiryAt: string | null;
  subscriptionToken: string;
  subToken: string;
  uploadBytes: number;
  downloadBytes: number;
  enforcementStatus: string;
}

export interface UserInput {
  inboundId: number;
  email: string;
  uuid: string;
  flow?: string;
  credentialKind?: string;
  password?: string;
  method?: string;
  auth?: string;
  subscriptionToken?: string;
  note?: string;
  enabled: boolean;
  trafficLimitBytes?: number | null;
  expiryAt?: string | null;
}

export interface LoginResponse {
  token: string;
  username: string;
}

export interface ApplyResult {
  revision: number;
  parentRevision: number;
  activeRevision: number | null;
  state: "active" | "activating" | "pendingMaintenance" | "failed";
  activationClass: "hotSwap" | "listenerHandover" | "maintenanceRequired";
  message: string;
}

export interface TrafficSnapshot {
  users: Array<{ email: string; uploadBytes: number; downloadBytes: number }>;
  inbounds: Array<{ tag: string; uploadBytes: number; downloadBytes: number }>;
}

export interface CapabilityItem {
  key: string;
  label: string;
  status: "supported" | "experimental" | "deprecated" | "unsupported";
  notes: string;
}

export interface CapabilityMap {
  protocols: CapabilityItem[];
  transports: CapabilityItem[];
  security: CapabilityItem[];
  config: CapabilityItem[];
  runtime: CapabilityItem[];
}

export interface ServiceStatus {
  systemdAvailable: boolean;
  activeState: string;
  subState: string;
  logs: string[];
}

export interface RevisionSummary {
  revision: number;
  parentRevision: number | null;
  actor: string;
  summary: string;
  activationClass: "hotSwap" | "listenerHandover" | "maintenanceRequired";
  createdAt: string;
}

export interface AppData {
  status: Status | null;
  settings: Settings | null;
  inbounds: Inbound[];
  outbounds: Outbound[];
  users: ManagedUser[];
  traffic: TrafficSnapshot;
  capabilities: CapabilityMap | null;
  service: ServiceStatus | null;
  revisions: RevisionSummary[];
}
