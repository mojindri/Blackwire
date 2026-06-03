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
});
