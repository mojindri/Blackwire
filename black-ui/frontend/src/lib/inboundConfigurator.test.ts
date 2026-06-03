import { describe, expect, it } from "vitest";
import {
  buildInboundInput,
  createInboundEditorState,
  replaceSlice,
  syncAfterStructuredChange,
  validateInboundState
} from "./inboundConfigurator";
import type { Inbound } from "./types";

function parseObject(raw?: string) {
  return raw?.trim() ? JSON.parse(raw) : {};
}

describe("inboundConfigurator", () => {
  it("preserves unknown keys while applying structured inbound edits", () => {
    const inbound: Inbound = {
      id: 1,
      tag: "vless-main",
      listen: "0.0.0.0",
      port: 443,
      protocol: "vless",
      enabled: true,
      transport: "ws",
      settings: JSON.stringify({
        decryption: "none",
        customSetting: "keep-me",
        clients: [{ id: "remove-me" }]
      }),
      streamSettings: JSON.stringify({
        network: "ws",
        security: "tls",
        customStream: "keep-stream",
        wsSettings: { path: "/old", headers: { Host: "old.example.com" } },
        tlsSettings: { serverName: "old.example.com", customTls: "keep-tls" }
      }),
      sniffing: JSON.stringify({
        enabled: true,
        destOverride: ["http"],
        customSniffing: "keep-sniff"
      }),
      limits: JSON.stringify({
        maxConnections: 200,
        customLimits: "keep-limits"
      }),
      createdAt: "",
      updatedAt: ""
    };

    const state = syncAfterStructuredChange({
      ...createInboundEditorState(inbound),
      wsPath: "/next",
      wsHost: "edge.example.com",
      tlsServerName: "new.example.com",
      sniffingDestOverride: ["http", "tls"],
      maxHandshakeSeconds: "15"
    });
    const built = buildInboundInput(state);
    const settings = parseObject(built.settings);
    const streamSettings = parseObject(built.streamSettings);
    const sniffing = parseObject(built.sniffing);
    const limits = parseObject(built.limits);

    expect(settings.customSetting).toBe("keep-me");
    expect(settings.clients).toBeUndefined();
    expect(streamSettings.customStream).toBe("keep-stream");
    expect(streamSettings.wsSettings.path).toBe("/next");
    expect(streamSettings.wsSettings.headers.Host).toBe("edge.example.com");
    expect(streamSettings.tlsSettings.serverName).toBe("new.example.com");
    expect(streamSettings.tlsSettings.customTls).toBe("keep-tls");
    expect(sniffing.customSniffing).toBe("keep-sniff");
    expect(sniffing.destOverride).toEqual(["http", "tls"]);
    expect(limits.customLimits).toBe("keep-limits");
    expect(limits.maxHandshakeSeconds).toBe(15);
  });

  it("reports invalid advanced JSON without dropping the editor text", () => {
    const state = replaceSlice(createInboundEditorState(), "streamSettings", "{invalid");

    expect(state.streamSettings.error).not.toBe("");
    expect(state.streamSettings.text).toBe("{invalid");
  });

  it("validates core inbound compatibility rules", () => {
    const validIssues = validateInboundState(createInboundEditorState());
    const invalidIssues = validateInboundState({
      ...createInboundEditorState(),
      listen: "example.com",
      port: 0,
      network: "ws",
      security: "reality"
    });

    expect(validIssues).toEqual([]);
    expect(invalidIssues.map((issue) => issue.field)).toEqual(["listen", "port", "security"]);
  });

  it("round-trips reality-specific fields into Blackwire-compatible stream settings", () => {
    const state = syncAfterStructuredChange({
      ...createInboundEditorState(),
      protocol: "vless",
      network: "tcp",
      security: "reality",
      realityServerName: "www.microsoft.com",
      realityPublicKey: "e1df9c8812b5ce9b3bd36da542896be856ad0a6c6e6df9d910a4040c07268142",
      realityShortId: "feedbeef",
      realityFingerprint: "chrome",
      realitySpiderX: "/"
    });
    const built = buildInboundInput(state);
    const streamSettings = parseObject(built.streamSettings);

    expect(built.transport).toBe("reality");
    expect(streamSettings.network).toBe("tcp");
    expect(streamSettings.security).toBe("reality");
    expect(streamSettings.realitySettings.publicKey).toBe("e1df9c8812b5ce9b3bd36da542896be856ad0a6c6e6df9d910a4040c07268142");
    expect(streamSettings.realitySettings.shortId).toBe("feedbeef");
    expect(streamSettings.realitySettings.shortIds).toEqual(["feedbeef"]);
    expect(streamSettings.realitySettings.serverName).toBe("www.microsoft.com");
    expect(streamSettings.realitySettings.fingerprint).toBe("chrome");
    expect(streamSettings.realitySettings.spiderX).toBe("/");
  });

  it("serializes websocket and TLS fields for common VLESS structured setups", () => {
    const state = syncAfterStructuredChange({
      ...createInboundEditorState(),
      protocol: "vless",
      network: "ws",
      security: "tls",
      wsPath: "/tls",
      wsHost: "tls.example.com",
      tlsServerName: "tls.example.com",
      tlsAlpn: "h2, http/1.1",
      tlsCertificateFile: "/etc/blackwire/fullchain.pem",
      tlsKeyFile: "/etc/blackwire/privkey.pem"
    });

    const built = buildInboundInput(state);
    const streamSettings = parseObject(built.streamSettings);

    expect(built.transport).toBe("ws");
    expect(streamSettings).toEqual({
      network: "ws",
      security: "tls",
      wsSettings: {
        path: "/tls",
        headers: {
          Host: "tls.example.com"
        }
      },
      tlsSettings: {
        serverName: "tls.example.com",
        alpn: ["h2", "http/1.1"],
        certificateFile: "/etc/blackwire/fullchain.pem",
        keyFile: "/etc/blackwire/privkey.pem"
      }
    });
  });

  it("serializes gRPC, HTTPUpgrade, and SplitHTTP transport helpers", () => {
    const grpcBuilt = buildInboundInput(
      syncAfterStructuredChange({
        ...createInboundEditorState(),
        network: "grpc",
        grpcServiceName: "GunService"
      })
    );
    const httpUpgradeBuilt = buildInboundInput(
      syncAfterStructuredChange({
        ...createInboundEditorState(),
        network: "httpupgrade",
        httpupgradePath: "/upgrade",
        httpupgradeHost: "edge.example.com"
      })
    );
    const splitHttpBuilt = buildInboundInput(
      syncAfterStructuredChange({
        ...createInboundEditorState(),
        network: "splithttp",
        splitHttpPath: "/packet"
      })
    );

    expect(parseObject(grpcBuilt.streamSettings)).toMatchObject({
      network: "grpc",
      grpcSettings: { serviceName: "GunService" }
    });
    expect(parseObject(httpUpgradeBuilt.streamSettings)).toMatchObject({
      network: "httpupgrade",
      httpupgradeSettings: { path: "/upgrade", host: "edge.example.com" }
    });
    expect(parseObject(splitHttpBuilt.streamSettings)).toMatchObject({
      network: "splithttp",
      splithttpSettings: { path: "/packet" }
    });
  });

  it("serializes sniffing and limits while clearing them when no longer needed", () => {
    const enabledState = syncAfterStructuredChange({
      ...createInboundEditorState(),
      sniffingEnabled: true,
      sniffingDestOverride: ["http", "tls"],
      sniffingMetadataOnly: true,
      maxConnections: "8000",
      maxHandshakeSeconds: "12"
    });
    const enabledBuilt = buildInboundInput(enabledState);

    expect(parseObject(enabledBuilt.sniffing)).toEqual({
      enabled: true,
      destOverride: ["http", "tls"],
      metadataOnly: true
    });
    expect(parseObject(enabledBuilt.limits)).toEqual({
      maxConnections: 8000,
      maxHandshakeSeconds: 12
    });

    const clearedBuilt = buildInboundInput(
      syncAfterStructuredChange({
        ...enabledState,
        sniffingEnabled: false,
        sniffingDestOverride: [],
        sniffingMetadataOnly: false,
        sniffingRouteOnly: false,
        maxConnections: "",
        maxHandshakeSeconds: ""
      })
    );

    expect(clearedBuilt.sniffing).toBe("");
    expect(clearedBuilt.limits).toBe("");
  });

  it("keeps Shadowsocks method only for shadowsocks protocol", () => {
    const ssBuilt = buildInboundInput(
      syncAfterStructuredChange({
        ...createInboundEditorState(),
        protocol: "shadowsocks",
        shadowsocksMethod: "2022-blake3-aes-128-gcm"
      })
    );
    const switchedBuilt = buildInboundInput(
      syncAfterStructuredChange({
        ...createInboundEditorState({
          id: 9,
          tag: "ss-main",
          listen: "0.0.0.0",
          port: 443,
          protocol: "shadowsocks",
          enabled: true,
          transport: "tcp",
          settings: JSON.stringify({ method: "2022-blake3-aes-128-gcm", extra: "keep-me" }),
          streamSettings: "{}",
          sniffing: "{}",
          limits: "{}",
          createdAt: "",
          updatedAt: ""
        }),
        protocol: "vless"
      })
    );

    expect(parseObject(ssBuilt.settings).method).toBe("2022-blake3-aes-128-gcm");
    expect(parseObject(switchedBuilt.settings).method).toBeUndefined();
    expect(parseObject(switchedBuilt.settings).extra).toBe("keep-me");
  });
});
