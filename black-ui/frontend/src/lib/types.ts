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

export interface CoreSettings {
  profile: "compat" | "fast" | "latency" | "throughput" | "badnet" | "mobile" | "stealth";
  fast: null | {
    strictProduction: boolean; pool: "adaptive" | "disabled" | "fixed"; splice: "adaptive" | "disabled" | "always";
    relay: { engine: "legacy" | "v2"; flush: "immediate" | "deferred" | "adaptive"; initialBuffer: number; maxBuffer: number };
    linux: { zerocopy: "disabled" | "bulk" | "always"; zerocopyMinBytes: number; ioUring: "disabled" | "auto" | "require"; afXdp: "disabled" | "auto" | "require" };
  };
  budget: null | { maxProtocolLayers: number; allowSniffing: boolean; allowFakeIp: boolean; maxRouteRules: number; maxHandshakeMs: number; preferDirectCopy: boolean; preferDatagramForUdp: boolean };
  vision: null | { directCopy: "auto" | "disabled" | "require"; maxPacketsToFilter: number; allowSpliceAfterDirect: boolean };
  firstPacketBoost: null | { enabled: boolean; dns: boolean; tlsClientHello: boolean; sendEarlyPayload: boolean; duplicateControlOnBadnet: boolean; priority: "normal" | "high" | "critical" };
  log: { level: string; json: boolean; file: string };
  metricsAddr: string | null;
  api: null | { listen: string; token: string | null; services: string[] };
  stats: null | { enabled: boolean };
  limits: {
    maxConnections: number | null;
    maxConnectionsPerInbound: number | null;
    maxConnectionsPerUser: number | null;
    maxHandshakeSeconds: number | null;
    maxIdleSeconds: number | null;
  };
  quic: null | { reusePort: boolean; endpoints: number | string; recvBufferBytes: number; sendBufferBytes: number; maxDatagramSize: number | string };
  datagram: null | { enabled: boolean; udpOverDatagram: boolean; tunPacketsOverDatagram: boolean; policy: "standard" | "h2-plus"; maxQueueDelayMs: number; fastDnsRetry: boolean; fastDnsRetryDelayMs: number };
  fec: null | { mode: "off" | "xor1-of-n" | "reed-solomon" | "raptor-like" | "auto"; maxOverheadPercent: number; protectClasses: string[]; avoidBulkTcp: boolean; disableForSequentialDns: boolean; minConcurrencyForBlockFec: number; maxGenerationPackets: number; maxGenerationDelayMs: number; recoveryDeadlineMs: number; dedupWindowPackets: number };
  tun: null | {
    name: string; address: string; netmask: string; mtu: number; bypassMark: number; outboundInterface: string | null; redirectPort: number; dnsPort: number; wintunFile: string | null;
    batch: { enabled: boolean; maxPackets: number; maxDelayUs: number; latencyFlushBytes: number };
    sessions: { udpMax: number; udpIdleTimeoutSec: number; tcpMax: number };
    linux: null | { backend: "tun" | "afxdp"; afXdp: { interface: string | null; queueId: number; ringEntries: number; frameCount: number; frameSize: number; forceCopy: boolean; forceZerocopy: boolean } };
  };
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
  transport?: string;
  security?: string;
  settings: string;
  streamSettings: string;
  sniffing: string;
  limits: string;
  createdAt?: string;
  updatedAt?: string;
}

export interface InboundInput {
  tag: string;
  listen: string;
  port: number;
  protocol: string;
  enabled: boolean;
  transport?: string;
  security?: string;
  settings?: string;
  streamSettings?: string;
  sniffing?: string;
  limits?: string;
}

export interface Outbound {
  id: number;
  tag: string;
  protocol: string;
  enabled: boolean;
  address?: string | null;
  port?: number | null;
  transport?: string;
  security?: string;
  settings: string;
  streamSettings: string;
  createdAt?: string;
  updatedAt?: string;
}

export interface OutboundInput {
  tag: string;
  protocol: string;
  enabled: boolean;
  address?: string | null;
  port?: number | null;
  transport?: string;
  security?: string;
  settings?: string;
  streamSettings?: string;
}

export interface ManagedUser {
  id: number;
  inboundId: number;
  email: string;
  uuid: string;
  flow: string;
  credentialKind?: string;
  credential: Record<string, unknown>;
  method?: string | null;
  note: string;
  enabled: boolean;
  trafficLimitBytes: number | null;
  expiryAt: string | null;
  subscriptionToken?: string;
  subToken: string;
  uploadBytes: number;
  downloadBytes: number;
  enforcementStatus: string;
  createdAt?: string;
  updatedAt?: string;
}

export interface UserInput {
  inboundId: number;
  email: string;
  uuid: string;
  flow?: string;
  credentialKind?: string;
  credential?: Record<string, unknown>;
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

export interface RouteInput {
  ruleType: string;
  port: string | null;
  outboundTag: string;
  domains: string[];
  ips: string[];
  inboundTags: string[];
  protocols: string[];
  users: string[];
}

export interface RoutingDns {
  domainStrategy: string;
  geoipFile: string | null;
  geositeFile: string | null;
  dnsServers: string[];
  fakeIpEnabled: boolean;
  fakeIpPool: string;
  rules: RouteInput[];
  balancers: BalancerInput[];
}

export interface BalancerInput {
  tag: string;
  strategy: string;
  members: Array<{ outboundTag: string; profileName: string | null }>;
  adaptive: null | { failureThreshold: number; cooldownSecs: number; ewmaAlpha: number; switchMargin: number };
  healthCheck: null | { url: string; intervalSecs: number; timeoutSecs: number; maxFailures: number };
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
  routingDns: RoutingDns;
  coreSettings: CoreSettings | null;
}
