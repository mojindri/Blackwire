import type { ConfigSection, Outbound } from "./types";

export type StructuredSectionName = "routing" | "dns" | "tun" | "api" | "metricsAddr" | "profile" | "fast";

export interface RoutingRuleEditor {
  type: string;
  domain: string;
  ip: string;
  port: string;
  inboundTag: string;
  protocol: string;
  user: string;
  outboundTag: string;
}

export interface RoutingBalancerProfileEditor {
  name: string;
  outboundTag: string;
}

export interface RoutingBalancerEditor {
  tag: string;
  selector: string;
  strategy: string;
  adaptiveFailureThreshold: string;
  adaptiveCooldownSecs: string;
  adaptiveEwmaAlpha: string;
  adaptiveSwitchMargin: string;
  healthUrl: string;
  healthIntervalSecs: string;
  healthTimeoutSecs: string;
  healthMaxFailures: string;
  profiles: RoutingBalancerProfileEditor[];
}

export interface DnsServerEditor {
  mode: "string" | "object";
  value: string;
  address: string;
  port: string;
  domains: string;
  expectedIPs: string;
  tag: string;
  clientIP: string;
  queryStrategy: string;
  skipFallback: boolean;
  finalQuery: boolean;
  disableCache: boolean;
  timeoutMs: string;
  serveStale: boolean;
  serveExpiredTTL: string;
}

export interface DnsHostEditor {
  domain: string;
  values: string;
}

export interface AdvancedConfigEditorState {
  name: string;
  enabled: boolean;
  rawText: string;
  rawError: string;
  advancedOpen: boolean;
  routingDomainStrategy: string;
  routingGeoipFile: string;
  routingGeositeFile: string;
  routingRules: RoutingRuleEditor[];
  routingBalancers: RoutingBalancerEditor[];
  dnsQueryStrategy: string;
  dnsClientIp: string;
  dnsDisableCache: boolean;
  dnsDisableFallback: boolean;
  dnsDisableFallbackIfMatch: boolean;
  dnsEnableParallelQuery: boolean;
  dnsUseSystemHosts: boolean;
  dnsServeStale: boolean;
  dnsServeExpiredTTL: string;
  dnsServers: DnsServerEditor[];
  dnsHosts: DnsHostEditor[];
  dnsFakeIpEnabled: boolean;
  dnsFakeIpPool: string;
  tunName: string;
  tunAddress: string;
  tunNetmask: string;
  tunMtu: string;
  tunBypassMark: string;
  tunRedirectPort: string;
  tunDnsPort: string;
  apiListen: string;
  metricsAddr: string;
  profile: string;
  profileCustom: string;
  fastStrictProduction: boolean;
  fastPool: string;
  fastSplice: string;
}

export interface AdvancedConfigValidationIssue {
  field: string;
  message: string;
}

const STRUCTURED_SECTIONS = new Set<StructuredSectionName>(["routing", "dns", "tun", "api", "metricsAddr", "profile", "fast"]);
const PROFILE_OPTIONS = new Set(["compat", "fast", "latency", "throughput", "badnet", "mobile", "stealth"]);

export function isStructuredSection(name: string): name is StructuredSectionName {
  return STRUCTURED_SECTIONS.has(name as StructuredSectionName);
}

export function createSectionEditorState(section: ConfigSection | null): AdvancedConfigEditorState {
  const name = section?.name ?? "";
  const enabled = section?.enabled ?? false;
  const rawText = section?.value ?? "{}";
  const parsed = parseSectionValue(rawText);
  const value = parsed.value;
  const profile = typeof value === "string" ? value : "";

  return {
    name,
    enabled,
    rawText,
    rawError: parsed.error,
    advancedOpen: false,
    routingDomainStrategy: stringValue(asObject(value).domainStrategy),
    routingGeoipFile: stringValue(asObject(value).geoipFile),
    routingGeositeFile: stringValue(asObject(value).geositeFile),
    routingRules: arrayValue(asObject(value).rules).map(toRoutingRuleEditor),
    routingBalancers: arrayValue(asObject(value).balancers).map(toRoutingBalancerEditor),
    dnsQueryStrategy: stringValue(asObject(value).queryStrategy),
    dnsClientIp: stringValue(asObject(value).clientIp),
    dnsDisableCache: boolValue(asObject(value).disableCache),
    dnsDisableFallback: boolValue(asObject(value).disableFallback),
    dnsDisableFallbackIfMatch: boolValue(asObject(value).disableFallbackIfMatch),
    dnsEnableParallelQuery: boolValue(asObject(value).enableParallelQuery),
    dnsUseSystemHosts: boolValue(asObject(value).useSystemHosts),
    dnsServeStale: boolValue(asObject(value).serveStale),
    dnsServeExpiredTTL: numberString(asObject(value).serveExpiredTTL),
    dnsServers: arrayValue(asObject(value).servers).map(toDnsServerEditor),
    dnsHosts: objectEntries(asObject(value).hosts).map(([domain, hostValue]) => ({
      domain,
      values: Array.isArray(hostValue) ? hostValue.filter((item) => typeof item === "string").join(", ") : typeof hostValue === "string" ? hostValue : ""
    })),
    dnsFakeIpEnabled: boolValue(asObject(asObject(value).fake_ip).enabled),
    dnsFakeIpPool: stringValue(asObject(asObject(value).fake_ip).pool),
    tunName: stringValue(asObject(value).name),
    tunAddress: stringValue(asObject(value).address),
    tunNetmask: stringValue(asObject(value).netmask),
    tunMtu: numberString(asObject(value).mtu),
    tunBypassMark: numberString(asObject(value).bypass_mark),
    tunRedirectPort: numberString(asObject(value).redirect_port),
    tunDnsPort: numberString(asObject(value).dns_port),
    apiListen: stringValue(asObject(value).listen),
    metricsAddr: typeof value === "string" ? value : stringValue(asObject(value).listen),
    profile: PROFILE_OPTIONS.has(profile) ? profile : "",
    profileCustom: PROFILE_OPTIONS.has(profile) ? "" : profile,
    fastStrictProduction: boolValue(asObject(value).strictProduction),
    fastPool: stringValue(asObject(value).pool),
    fastSplice: stringValue(asObject(value).splice)
  };
}

export function replaceSectionJson(state: AdvancedConfigEditorState, rawText: string): AdvancedConfigEditorState {
  const parsed = parseSectionValue(rawText);
  if (parsed.error) {
    return { ...state, rawText, rawError: parsed.error };
  }
  const next = createSectionEditorState({
    name: state.name,
    enabled: state.enabled,
    value: rawText,
    updatedAt: ""
  });
  return { ...next, advancedOpen: state.advancedOpen };
}

export function syncSectionState(state: AdvancedConfigEditorState): AdvancedConfigEditorState {
  const parsed = parseSectionValue(state.rawText);
  if (parsed.error) {
    return { ...state, rawError: parsed.error };
  }
  const next = buildSectionObject(state, parsed.value);
  return {
    ...state,
    rawText: stringifySectionValue(state.name, next),
    rawError: ""
  };
}

export function buildSectionValue(state: AdvancedConfigEditorState): string {
  const parsed = parseSectionValue(state.rawText);
  const base = parsed.error ? defaultValueForSection(state.name) : parsed.value;
  return stringifySectionValue(state.name, buildSectionObject(state, base));
}

export function validateSectionState(state: AdvancedConfigEditorState): AdvancedConfigValidationIssue[] {
  const issues: AdvancedConfigValidationIssue[] = [];
  if (state.rawError) issues.push({ field: "raw", message: "Advanced JSON is invalid." });

  if (state.name === "routing") {
    state.routingRules.forEach((rule, index) => {
      if (!rule.outboundTag.trim()) {
        issues.push({ field: `routingRules.${index}`, message: "Each routing rule needs an outbound tag." });
      }
    });
    state.routingBalancers.forEach((balancer, index) => {
      if (!balancer.tag.trim()) {
        issues.push({ field: `routingBalancers.${index}`, message: "Each balancer needs a tag." });
      }
      if (!csvValues(balancer.selector).length) {
        issues.push({ field: `routingBalancers.${index}`, message: "Each balancer needs at least one selector outbound." });
      }
    });
  }

  if (state.name === "dns") {
    state.dnsServers.forEach((server, index) => {
      if (server.mode === "string") {
        if (!server.value.trim()) issues.push({ field: `dnsServers.${index}`, message: "DNS server entries cannot be empty." });
      } else {
        issues.push({ field: `dnsServers.${index}`, message: "Structured DNS currently supports string server entries only." });
      }
    });
  }

  if (state.name === "tun") {
    if (!state.tunName.trim()) issues.push({ field: "tunName", message: "TUN name is required." });
    if (!state.tunAddress.trim()) issues.push({ field: "tunAddress", message: "TUN address is required." });
  }

  if (state.name === "api" && !state.apiListen.trim()) {
    issues.push({ field: "apiListen", message: "API listen address is required." });
  }
  if (state.name === "metricsAddr" && !state.metricsAddr.trim()) {
    issues.push({ field: "metricsAddr", message: "Metrics address is required." });
  }
  if (state.name === "profile" && !profileValue(state).trim()) {
    issues.push({ field: "profile", message: "Profile value is required." });
  }
  if (state.name === "fast") {
    if (!state.fastPool.trim()) issues.push({ field: "fastPool", message: "Fast pool mode is required." });
    if (!state.fastSplice.trim()) issues.push({ field: "fastSplice", message: "Fast splice mode is required." });
  }
  return issues;
}

export function applyAdaptiveRoutingTemplate(state: AdvancedConfigEditorState, outbounds: Outbound[]): AdvancedConfigEditorState {
  const enabledOutbounds = outbounds.filter((outbound) => outbound.enabled).slice(0, 2);
  if (enabledOutbounds.length < 2) return state;
  const [primary, backup] = enabledOutbounds;
  return syncSectionState({
    ...state,
    enabled: true,
    routingRules: [{ type: "field", domain: "", ip: "", port: "", inboundTag: "", protocol: "", user: "", outboundTag: "auto-proxy" }],
    routingBalancers: [
      {
        tag: "auto-proxy",
        selector: `${primary.tag}, ${backup.tag}`,
        strategy: "adaptive",
        adaptiveFailureThreshold: "2",
        adaptiveCooldownSecs: "30",
        adaptiveEwmaAlpha: "0.2",
        adaptiveSwitchMargin: "0.15",
        healthUrl: "http://www.gstatic.com/generate_204",
        healthIntervalSecs: "30",
        healthTimeoutSecs: "5",
        healthMaxFailures: "2",
        profiles: [
          { name: "stable", outboundTag: primary.tag },
          { name: "backup", outboundTag: backup.tag }
        ]
      }
    ]
  });
}

function buildSectionObject(state: AdvancedConfigEditorState, base: unknown): unknown {
  if (state.name === "metricsAddr") {
    return state.metricsAddr.trim();
  }
  if (state.name === "profile") {
    return profileValue(state).trim();
  }

  const root = asObject(base);
  if (state.name === "routing") {
    setOrDelete(root, "domainStrategy", state.routingDomainStrategy.trim());
    setOrDelete(root, "geoipFile", state.routingGeoipFile.trim());
    setOrDelete(root, "geositeFile", state.routingGeositeFile.trim());
    root.rules = state.routingRules.map((rule) => {
      const next = asObject({});
      setOrDelete(next, "type", rule.type.trim() || "field");
      setOrDelete(next, "domain", csvValues(rule.domain));
      setOrDelete(next, "ip", csvValues(rule.ip));
      setOrDelete(next, "port", rule.port.trim());
      setOrDelete(next, "inboundTag", csvValues(rule.inboundTag));
      setOrDelete(next, "protocol", csvValues(rule.protocol));
      setOrDelete(next, "user", csvValues(rule.user));
      setOrDelete(next, "outboundTag", rule.outboundTag.trim());
      return next;
    });
    root.balancers = state.routingBalancers.map((balancer) => {
      const next = asObject({});
      setOrDelete(next, "tag", balancer.tag.trim());
      setOrDelete(next, "selector", csvValues(balancer.selector));
      const profiles = balancer.profiles
        .filter((item) => item.name.trim() && item.outboundTag.trim())
        .map((item) => ({
          name: item.name.trim(),
          outboundTag: item.outboundTag.trim()
        }));
      if (profiles.length > 0) next.profiles = profiles;
      else delete next.profiles;
      if (balancer.strategy.trim()) next.strategy = balancer.strategy.trim();
      const adaptive = asObject(next.adaptive);
      setOrDeleteNumber(adaptive, "failureThreshold", balancer.adaptiveFailureThreshold);
      setOrDeleteNumber(adaptive, "cooldownSecs", balancer.adaptiveCooldownSecs);
      setOrDeleteNumber(adaptive, "ewmaAlpha", balancer.adaptiveEwmaAlpha);
      setOrDeleteNumber(adaptive, "switchMargin", balancer.adaptiveSwitchMargin);
      if (Object.keys(adaptive).length > 0) next.adaptive = adaptive;
      else delete next.adaptive;
      const health = asObject(next.health_check);
      setOrDelete(health, "url", balancer.healthUrl.trim());
      setOrDeleteNumber(health, "interval_secs", balancer.healthIntervalSecs);
      setOrDeleteNumber(health, "timeout_secs", balancer.healthTimeoutSecs);
      setOrDeleteNumber(health, "max_failures", balancer.healthMaxFailures);
      if (Object.keys(health).length > 0) next.health_check = health;
      else delete next.health_check;
      return next;
    });
    return root;
  }
  if (state.name === "dns") {
    root.servers = state.dnsServers.map((server) => server.value.trim()).filter(Boolean);
    const fakeIp = asObject(root.fake_ip);
    setOrDeleteBool(fakeIp, "enabled", state.dnsFakeIpEnabled);
    setOrDelete(fakeIp, "pool", state.dnsFakeIpPool.trim());
    if (Object.keys(fakeIp).length > 0) root.fake_ip = fakeIp;
    else delete root.fake_ip;
    delete root.hosts;
    delete root.queryStrategy;
    delete root.clientIp;
    delete root.disableCache;
    delete root.disableFallback;
    delete root.disableFallbackIfMatch;
    delete root.enableParallelQuery;
    delete root.useSystemHosts;
    delete root.serveStale;
    delete root.serveExpiredTTL;
    return root;
  }
  if (state.name === "tun") {
    setOrDelete(root, "name", state.tunName.trim());
    setOrDelete(root, "address", state.tunAddress.trim());
    setOrDelete(root, "netmask", state.tunNetmask.trim());
    setOrDeleteNumber(root, "mtu", state.tunMtu);
    setOrDeleteNumber(root, "bypass_mark", state.tunBypassMark);
    setOrDeleteNumber(root, "redirect_port", state.tunRedirectPort);
    setOrDeleteNumber(root, "dns_port", state.tunDnsPort);
    return root;
  }
  if (state.name === "api") {
    setOrDelete(root, "listen", state.apiListen.trim());
    return root;
  }
  if (state.name === "fast") {
    root.strictProduction = state.fastStrictProduction;
    setOrDelete(root, "pool", state.fastPool.trim());
    setOrDelete(root, "splice", state.fastSplice.trim());
    return root;
  }
  return base;
}

function defaultValueForSection(name: string): unknown {
  if (name === "metricsAddr" || name === "profile") return "";
  return {};
}

function parseSectionValue(rawText: string): { value: unknown; error: string } {
  const trimmed = rawText.trim();
  if (!trimmed) return { value: {}, error: "" };
  try {
    return { value: JSON.parse(trimmed), error: "" };
  } catch (error) {
    return { value: {}, error: error instanceof Error ? error.message : "Invalid JSON." };
  }
}

function stringifySectionValue(name: string, value: unknown): string {
  if (name === "metricsAddr" || name === "profile") return JSON.stringify(value, null, 2);
  return JSON.stringify(value, null, 2);
}

function toRoutingRuleEditor(value: unknown): RoutingRuleEditor {
  const rule = asObject(value);
  return {
    type: stringValue(rule.type) || "field",
    domain: csvText(rule.domain),
    ip: csvText(rule.ip),
    port: scalarText(rule.port),
    inboundTag: csvText(rule.inboundTag),
    protocol: csvText(rule.protocol),
    user: csvText(rule.user),
    outboundTag: stringValue(rule.outboundTag)
  };
}

function toRoutingBalancerEditor(value: unknown): RoutingBalancerEditor {
  const balancer = asObject(value);
  const adaptive = asObject(balancer.adaptive);
  const health = asObject(balancer.health_check);
  return {
    tag: stringValue(balancer.tag),
    selector: csvText(balancer.selector),
    strategy: stringValue(balancer.strategy),
    adaptiveFailureThreshold: numberString(adaptive.failureThreshold),
    adaptiveCooldownSecs: numberString(adaptive.cooldownSecs),
    adaptiveEwmaAlpha: scalarText(adaptive.ewmaAlpha),
    adaptiveSwitchMargin: scalarText(adaptive.switchMargin),
    healthUrl: stringValue(health.url),
    healthIntervalSecs: numberString(health.interval_secs),
    healthTimeoutSecs: numberString(health.timeout_secs),
    healthMaxFailures: numberString(health.max_failures),
    profiles: arrayValue(balancer.profiles).map((profile) => {
      const next = asObject(profile);
      return { name: stringValue(next.name), outboundTag: stringValue(next.outboundTag) };
    })
  };
}

function toDnsServerEditor(value: unknown): DnsServerEditor {
  if (typeof value === "string") {
    return {
      mode: "string",
      value,
      address: "",
      port: "",
      domains: "",
      expectedIPs: "",
      tag: "",
      clientIP: "",
      queryStrategy: "",
      skipFallback: false,
      finalQuery: false,
      disableCache: false,
      timeoutMs: "",
      serveStale: false,
      serveExpiredTTL: ""
    };
  }
  const server = asObject(value);
  return {
    mode: "object",
    value: "",
    address: stringValue(server.address),
    port: numberString(server.port),
    domains: csvText(server.domains),
    expectedIPs: csvText(server.expectedIPs ?? server.expectIPs),
    tag: stringValue(server.tag),
    clientIP: stringValue(server.clientIP),
    queryStrategy: stringValue(server.queryStrategy),
    skipFallback: boolValue(server.skipFallback),
    finalQuery: boolValue(server.finalQuery),
    disableCache: boolValue(server.disableCache),
    timeoutMs: numberString(server.timeoutMs),
    serveStale: boolValue(server.serveStale),
    serveExpiredTTL: numberString(server.serveExpiredTTL)
  };
}

function asObject(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value) ? { ...(value as Record<string, unknown>) } : {};
}

function arrayValue(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function objectEntries(value: unknown): Array<[string, unknown]> {
  return value && typeof value === "object" && !Array.isArray(value) ? Object.entries(value as Record<string, unknown>) : [];
}

function stringValue(value: unknown): string {
  return typeof value === "string" ? value : "";
}

function boolValue(value: unknown): boolean {
  return value === true;
}

function numberString(value: unknown): string {
  return typeof value === "number" ? String(value) : "";
}

function scalarText(value: unknown): string {
  if (typeof value === "string") return value;
  if (typeof value === "number") return String(value);
  return "";
}

function csvText(value: unknown): string {
  if (Array.isArray(value)) return value.filter((item) => typeof item === "string").join(", ");
  if (typeof value === "string") return value;
  return "";
}

function csvValues(value: string): string[] {
  return value
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);
}

function setOrDelete(target: Record<string, unknown>, key: string, value: string | string[]) {
  if (Array.isArray(value)) {
    if (value.length > 0) target[key] = value;
    else delete target[key];
    return;
  }
  if (value) target[key] = value;
  else delete target[key];
}

function setOrDeleteBool(target: Record<string, unknown>, key: string, value: boolean) {
  if (value) target[key] = true;
  else delete target[key];
}

function setOrDeleteNumber(target: Record<string, unknown>, key: string, raw: string) {
  const trimmed = raw.trim();
  if (!trimmed) {
    delete target[key];
    return;
  }
  const parsed = Number(trimmed);
  if (Number.isFinite(parsed)) target[key] = parsed;
  else delete target[key];
}

function profileValue(state: AdvancedConfigEditorState): string {
  return state.profile || state.profileCustom;
}
