import { describe, expect, it } from "vitest";
import {
  buildOutboundInput,
  createOutboundEditorState,
  replaceOutboundSlice,
  syncOutboundAfterStructuredChange,
  validateOutboundState
} from "./outboundConfigurator";
import type { Outbound } from "./types";

function parseObject(raw?: string) {
  return raw?.trim() ? JSON.parse(raw) : {};
}

describe("outboundConfigurator", () => {
  it("preserves unknown keys while applying structured outbound edits", () => {
    const outbound: Outbound = {
      id: 1,
      tag: "proxy-a",
      protocol: "vless",
      enabled: true,
      settings: JSON.stringify({
        address: "127.0.0.1",
        port: 443,
        users: [{ id: "459dc0c8-d891-4768-9234-faf11fd26b5d", flow: "xtls-rprx-vision" }],
        customSetting: "keep-me"
      }),
      streamSettings: JSON.stringify({
        network: "ws",
        security: "tls",
        customStream: "keep-stream",
        wsSettings: { path: "/old", headers: { Host: "old.example.com" } },
        tlsSettings: { serverName: "old.example.com", customTls: "keep-tls" }
      }),
      createdAt: "",
      updatedAt: ""
    };

    const state = syncOutboundAfterStructuredChange({
      ...createOutboundEditorState(outbound),
      address: "127.0.0.2",
      wsPath: "/next",
      wsHost: "edge.example.com",
      tlsServerName: "new.example.com"
    });
    const built = buildOutboundInput(state);
    const settings = parseObject(built.settings);
    const streamSettings = parseObject(built.streamSettings);

    expect(settings.address).toBe("127.0.0.2");
    expect(settings.customSetting).toBe("keep-me");
    expect(settings.users[0].id).toBe("459dc0c8-d891-4768-9234-faf11fd26b5d");
    expect(settings.users[0].flow).toBe("xtls-rprx-vision");
    expect(streamSettings.customStream).toBe("keep-stream");
    expect(streamSettings.wsSettings.path).toBe("/next");
    expect(streamSettings.wsSettings.headers.Host).toBe("edge.example.com");
    expect(streamSettings.tlsSettings.serverName).toBe("new.example.com");
    expect(streamSettings.tlsSettings.customTls).toBe("keep-tls");
  });

  it("reports invalid advanced JSON without dropping the editor text", () => {
    const state = replaceOutboundSlice(createOutboundEditorState(), "settings", "{invalid");

    expect(state.settings.error).not.toBe("");
    expect(state.settings.text).toBe("{invalid");
  });

  it("validates protocol-specific outbound requirements", () => {
    const vlessIssues = validateOutboundState({
      ...createOutboundEditorState(),
      protocol: "vless",
      address: "127.0.0.1",
      port: "443",
      userId: ""
    });
    const hysteriaIssues = validateOutboundState({
      ...createOutboundEditorState(),
      protocol: "hysteria2",
      server: "example.com:443"
    });
    const disabledIssues = validateOutboundState({
      ...createOutboundEditorState(),
      protocol: "trojan",
      enabled: false,
      address: "",
      port: "",
      password: ""
    });

    expect(vlessIssues.map((issue) => issue.field)).toContain("userId");
    expect(hysteriaIssues.map((issue) => issue.field)).toContain("server");
    expect(disabledIssues).toEqual([]);
  });

  it("round-trips hysteria2 settings without leaking unrelated structured fields", () => {
    const outbound: Outbound = {
      id: 2,
      tag: "hy2-main",
      protocol: "hysteria2",
      enabled: true,
      settings: JSON.stringify({
        server: "127.0.0.1:443",
        auth: "shared-secret",
        customSetting: "keep-me"
      }),
      streamSettings: JSON.stringify({
        network: "tcp",
        security: "tls",
        tlsSettings: { serverName: "old.example.com", customTls: "keep-tls" }
      }),
      createdAt: "",
      updatedAt: ""
    };

    const state = syncOutboundAfterStructuredChange({
      ...createOutboundEditorState(outbound),
      server: "127.0.0.2:8443",
      tlsServerName: "new.example.com",
      address: "127.0.0.9",
      port: "9000"
    });
    const built = buildOutboundInput(state);
    const settings = parseObject(built.settings);
    const streamSettings = parseObject(built.streamSettings);

    expect(settings.server).toBe("127.0.0.2:8443");
    expect(settings.auth).toBe("shared-secret");
    expect(settings.customSetting).toBe("keep-me");
    expect(settings.address).toBeUndefined();
    expect(settings.port).toBeUndefined();
    expect(streamSettings.tlsSettings.serverName).toBe("new.example.com");
    expect(streamSettings.tlsSettings.customTls).toBe("keep-tls");
  });
});
