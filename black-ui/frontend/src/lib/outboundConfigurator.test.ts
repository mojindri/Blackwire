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
  it("serializes the supported outbound protocol matrix without leaking unrelated fields", () => {
    const cases = [
      {
        protocol: "freedom",
        patch: {},
        expectedSettings: {}
      },
      {
        protocol: "vless",
        patch: { address: "127.0.0.1", port: "443", userId: "459dc0c8-d891-4768-9234-faf11fd26b5d" },
        expectedSettings: { address: "127.0.0.1", port: 443, users: [{ id: "459dc0c8-d891-4768-9234-faf11fd26b5d" }] }
      },
      {
        protocol: "vmess",
        patch: { address: "127.0.0.1", port: "444", userId: "8f1edb46-6bb1-447f-a5de-2d86bb8822cc" },
        expectedSettings: { address: "127.0.0.1", port: 444, users: [{ id: "8f1edb46-6bb1-447f-a5de-2d86bb8822cc" }] }
      },
      {
        protocol: "trojan",
        patch: { address: "127.0.0.1", port: "445", password: "secret" },
        expectedSettings: { address: "127.0.0.1", port: 445, password: "secret" }
      },
      {
        protocol: "shadowsocks",
        patch: { address: "127.0.0.1", port: "446", password: "secret", method: "2022-blake3-aes-128-gcm" },
        expectedSettings: { address: "127.0.0.1", port: 446, password: "secret", method: "2022-blake3-aes-128-gcm" }
      },
      {
        protocol: "hysteria2",
        patch: { server: "127.0.0.1:8443" },
        expectedSettings: { server: "127.0.0.1:8443" }
      }
    ] as const;

    for (const testCase of cases) {
      const built = buildOutboundInput(
        syncOutboundAfterStructuredChange({
          ...createOutboundEditorState(),
          protocol: testCase.protocol,
          ...testCase.patch
        })
      );
      const settings = parseObject(built.settings);

      expect(built.protocol, testCase.protocol).toBe(testCase.protocol);
      expect(settings, testCase.protocol).toMatchObject(testCase.expectedSettings);
      if (!["vless", "vmess"].includes(testCase.protocol)) {
        expect(settings.users, `${testCase.protocol} users leak`).toBeUndefined();
      }
      if (testCase.protocol !== "shadowsocks") {
        expect(settings.method, `${testCase.protocol} method leak`).toBeUndefined();
      }
      if (testCase.protocol !== "hysteria2") {
        expect(settings.server, `${testCase.protocol} server leak`).toBeUndefined();
      }
    }
  });

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

  it("rejects invalid address and port inputs for proxy-style outbounds", () => {
    const issues = validateOutboundState({
      ...createOutboundEditorState(),
      protocol: "trojan",
      address: "example.com",
      port: "70000",
      password: "secret"
    });

    expect(issues.map((issue) => issue.field)).toEqual(["address", "port"]);
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

  it("serializes KCP tuning and preserves QUIC network selection", () => {
    const kcpBuilt = buildOutboundInput(
      syncOutboundAfterStructuredChange({
        ...createOutboundEditorState(),
        protocol: "vless",
        address: "127.0.0.1",
        port: "443",
        userId: "459dc0c8-d891-4768-9234-faf11fd26b5d",
        network: "kcp",
        kcpHeader: "srtp",
        kcpMtu: "1350",
        kcpTti: "20",
        kcpUplinkCapacity: "5",
        kcpDownlinkCapacity: "20",
        kcpCongestion: true,
        kcpReadBufferSize: "2",
        kcpWriteBufferSize: "2"
      })
    );
    const quicBuilt = buildOutboundInput(
      syncOutboundAfterStructuredChange({
        ...createOutboundEditorState(),
        protocol: "vless",
        address: "127.0.0.1",
        port: "443",
        userId: "459dc0c8-d891-4768-9234-faf11fd26b5d",
        network: "quic",
        security: "tls",
        tlsServerName: "quic.example.com"
      })
    );

    expect(parseObject(kcpBuilt.streamSettings)).toMatchObject({
      network: "kcp",
      security: "none",
      kcpSettings: {
        header: "srtp",
        mtu: 1350,
        tti: 20,
        uplink_capacity: 5,
        downlink_capacity: 20,
        congestion: true,
        read_buffer_size: 2,
        write_buffer_size: 2
      }
    });
    expect(parseObject(quicBuilt.streamSettings)).toMatchObject({
      network: "quic",
      security: "tls",
      tlsSettings: { serverName: "quic.example.com" }
    });
  });

  it("covers the structured outbound transport and security matrix", () => {
    const base = {
      ...createOutboundEditorState(),
      protocol: "vless",
      address: "127.0.0.1",
      port: "443",
      userId: "459dc0c8-d891-4768-9234-faf11fd26b5d"
    };
    const cases = [
      {
        label: "tcp-none",
        patch: { network: "tcp", security: "none" },
        expected: { network: "tcp", security: "none" }
      },
      {
        label: "ws-none",
        patch: { network: "ws", security: "none", wsPath: "/ws", wsHost: "ws.example.com" },
        expected: { network: "ws", security: "none", wsSettings: { path: "/ws", headers: { Host: "ws.example.com" } } }
      },
      {
        label: "grpc-none",
        patch: { network: "grpc", security: "none", grpcServiceName: "GunService" },
        expected: { network: "grpc", security: "none", grpcSettings: { serviceName: "GunService" } }
      },
      {
        label: "httpupgrade-none",
        patch: { network: "httpupgrade", security: "none", httpupgradePath: "/upgrade", httpupgradeHost: "edge.example.com" },
        expected: { network: "httpupgrade", security: "none", httpupgradeSettings: { path: "/upgrade", host: "edge.example.com" } }
      },
      {
        label: "splithttp-none",
        patch: { network: "splithttp", security: "none", splitHttpPath: "/packet" },
        expected: { network: "splithttp", security: "none", splithttpSettings: { path: "/packet" } }
      },
      {
        label: "kcp-none",
        patch: { network: "kcp", security: "none", kcpHeader: "srtp", kcpMtu: "1350" },
        expected: { network: "kcp", security: "none", kcpSettings: { header: "srtp", mtu: 1350 } }
      },
      {
        label: "quic-tls",
        patch: { network: "quic", security: "tls", tlsServerName: "quic.example.com" },
        expected: { network: "quic", security: "tls", tlsSettings: { serverName: "quic.example.com" } }
      }
    ] as const;

    for (const testCase of cases) {
      const built = buildOutboundInput(syncOutboundAfterStructuredChange({ ...base, ...testCase.patch }));
      expect(parseObject(built.streamSettings), testCase.label).toMatchObject(testCase.expected);
    }
  });
});
