import { describe, expect, it } from "vitest";
import {
  applyAdaptiveRoutingTemplate,
  buildSectionValue,
  createSectionEditorState,
  replaceSectionJson,
  validateSectionState
} from "./advancedConfigConfigurator";
import type { ConfigSection, Outbound } from "./types";

function parseValue(raw: string) {
  return JSON.parse(raw);
}

describe("advancedConfigConfigurator", () => {
  it("serializes DNS to the Blackwire-supported structured shape", () => {
    const section: ConfigSection = {
      name: "dns",
      enabled: true,
      value: JSON.stringify({
        servers: ["1.1.1.1"],
        queryStrategy: "UseIPv4",
        hosts: { "example.com": "1.1.1.1" },
        fake_ip: { enabled: false, pool: "198.18.0.0/15" }
      }),
      updatedAt: ""
    };

    const state = {
      ...createSectionEditorState(section),
      dnsServers: [
        { mode: "string" as const, value: "8.8.8.8", address: "", port: "", domains: "", expectedIPs: "", tag: "", clientIP: "", queryStrategy: "", skipFallback: false, finalQuery: false, disableCache: false, timeoutMs: "", serveStale: false, serveExpiredTTL: "" }
      ],
      dnsFakeIpEnabled: true,
      dnsFakeIpPool: "198.18.0.0/15"
    };

    const built = parseValue(buildSectionValue(state));

    expect(built).toEqual({
      servers: ["8.8.8.8"],
      fake_ip: {
        enabled: true,
        pool: "198.18.0.0/15"
      }
    });
  });

  it("rejects unsupported structured DNS object server entries", () => {
    const state = {
      ...createSectionEditorState({
        name: "dns",
        enabled: true,
        value: JSON.stringify({ servers: [] }),
        updatedAt: ""
      }),
      dnsServers: [
        { mode: "object" as const, value: "", address: "https://1.1.1.1/dns-query", port: "443", domains: "", expectedIPs: "", tag: "", clientIP: "", queryStrategy: "", skipFallback: false, finalQuery: false, disableCache: false, timeoutMs: "", serveStale: false, serveExpiredTTL: "" }
      ]
    };

    expect(validateSectionState(state).map((issue) => issue.field)).toContain("dnsServers.0");
  });

  it("serializes schema-supported routing fields only", () => {
    const state = {
      ...createSectionEditorState({
        name: "routing",
        enabled: true,
        value: JSON.stringify({ rules: [], balancers: [], geoipFile: "old-geoip.dat", geositeFile: "old-geosite.dat" }),
        updatedAt: ""
      }),
      routingGeoipFile: "geoip.dat",
      routingGeositeFile: "geosite.dat",
      routingRules: [
        {
          type: "field",
          domain: "geosite:google, example.com",
          ip: "geoip:private",
          port: "443",
          inboundTag: "vless-main",
          protocol: "http,tls",
          user: "alice@example.com",
          outboundTag: "auto-proxy"
        }
      ],
      routingBalancers: [
        {
          tag: "auto-proxy",
          selector: "proxy-a, proxy-b",
          strategy: "adaptive",
          adaptiveFailureThreshold: "2",
          adaptiveCooldownSecs: "30",
          adaptiveEwmaAlpha: "0.2",
          adaptiveSwitchMargin: "0.15",
          healthUrl: "http://www.gstatic.com/generate_204",
          healthIntervalSecs: "30",
          healthTimeoutSecs: "5",
          healthMaxFailures: "2",
          profiles: [{ name: "stable", outboundTag: "proxy-a" }]
        }
      ]
    };

    const built = parseValue(buildSectionValue(state));

    expect(built.geoipFile).toBe("geoip.dat");
    expect(built.geositeFile).toBe("geosite.dat");
    expect(built.rules).toEqual([
      {
        type: "field",
        domain: ["geosite:google", "example.com"],
        ip: ["geoip:private"],
        port: "443",
        inboundTag: ["vless-main"],
        protocol: ["http", "tls"],
        user: ["alice@example.com"],
        outboundTag: "auto-proxy"
      }
    ]);
    expect(built.balancers).toEqual([
      {
        tag: "auto-proxy",
        selector: ["proxy-a", "proxy-b"],
        strategy: "adaptive",
        profiles: [{ name: "stable", outboundTag: "proxy-a" }],
        adaptive: {
          failureThreshold: 2,
          cooldownSecs: 30,
          ewmaAlpha: 0.2,
          switchMargin: 0.15
        },
        health_check: {
          url: "http://www.gstatic.com/generate_204",
          interval_secs: 30,
          timeout_secs: 5,
          max_failures: 2
        }
      }
    ]);
  });

  it("recognizes the expanded profile mode set", () => {
    const state = createSectionEditorState({
      name: "profile",
      enabled: true,
      value: JSON.stringify("latency"),
      updatedAt: ""
    });

    expect(state.profile).toBe("latency");
    expect(state.profileCustom).toBe("");
    expect(buildSectionValue(state)).toBe(JSON.stringify("latency", null, 2));
  });

  it("preserves explicit false for fast.strictProduction", () => {
    const state = {
      ...createSectionEditorState({
        name: "fast",
        enabled: true,
        value: JSON.stringify({ strictProduction: true, pool: "disabled", splice: "adaptive" }),
        updatedAt: ""
      }),
      fastStrictProduction: false,
      fastPool: "adaptive",
      fastSplice: "always"
    };

    expect(parseValue(buildSectionValue(state))).toEqual({
      strictProduction: false,
      pool: "adaptive",
      splice: "always"
    });
  });

  it("serializes api, metrics, and tun structured sections", () => {
    const apiState = {
      ...createSectionEditorState({
        name: "api",
        enabled: true,
        value: JSON.stringify({ listen: "127.0.0.1:62789" }),
        updatedAt: ""
      }),
      apiListen: "127.0.0.1:62790"
    };
    const metricsState = {
      ...createSectionEditorState({
        name: "metricsAddr",
        enabled: true,
        value: JSON.stringify("127.0.0.1:9090"),
        updatedAt: ""
      }),
      metricsAddr: "127.0.0.1:19090"
    };
    const tunState = {
      ...createSectionEditorState({
        name: "tun",
        enabled: true,
        value: JSON.stringify({}),
        updatedAt: ""
      }),
      tunName: "blackwire-tun",
      tunAddress: "198.18.0.1",
      tunNetmask: "255.255.0.0",
      tunMtu: "1500",
      tunBypassMark: "4660",
      tunRedirectPort: "7890",
      tunDnsPort: "5300"
    };

    expect(parseValue(buildSectionValue(apiState))).toEqual({ listen: "127.0.0.1:62790" });
    expect(parseValue(buildSectionValue(metricsState))).toBe("127.0.0.1:19090");
    expect(parseValue(buildSectionValue(tunState))).toEqual({
      name: "blackwire-tun",
      address: "198.18.0.1",
      netmask: "255.255.0.0",
      mtu: 1500,
      bypass_mark: 4660,
      redirect_port: 7890,
      dns_port: 5300
    });
  });

  it("keeps invalid advanced JSON text while surfacing an error", () => {
    const state = replaceSectionJson(
      createSectionEditorState({
        name: "log",
        enabled: true,
        value: JSON.stringify({ level: "info" }),
        updatedAt: ""
      }),
      "{invalid"
    );

    expect(state.rawText).toBe("{invalid");
    expect(state.rawError).not.toBe("");
  });

  it("builds the adaptive routing template from the first two enabled outbounds", () => {
    const outbounds: Outbound[] = [
      { id: 1, tag: "proxy-a", protocol: "vless", enabled: true, settings: "{}", streamSettings: "{}", createdAt: "", updatedAt: "" },
      { id: 2, tag: "proxy-b", protocol: "trojan", enabled: true, settings: "{}", streamSettings: "{}", createdAt: "", updatedAt: "" },
      { id: 3, tag: "proxy-c", protocol: "freedom", enabled: false, settings: "{}", streamSettings: "{}", createdAt: "", updatedAt: "" }
    ];

    const templated = applyAdaptiveRoutingTemplate(
      createSectionEditorState({
        name: "routing",
        enabled: false,
        value: JSON.stringify({ rules: [], balancers: [] }),
        updatedAt: ""
      }),
      outbounds
    );
    const built = parseValue(buildSectionValue(templated));

    expect(templated.enabled).toBe(true);
    expect(built.rules[0].outboundTag).toBe("auto-proxy");
    expect(built.balancers[0].selector).toEqual(["proxy-a", "proxy-b"]);
    expect(built.balancers[0].profiles).toEqual([
      { name: "stable", outboundTag: "proxy-a" },
      { name: "backup", outboundTag: "proxy-b" }
    ]);
  });
});
