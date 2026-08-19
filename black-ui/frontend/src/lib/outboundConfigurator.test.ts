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
      },
      {
        protocol: "tuic",
        patch: { server: "127.0.0.1:8444", userId: "8b9a2f4a-5e51-47a6-b012-75c9dfe8bc30", password: "secret" },
        expectedSettings: { server: "127.0.0.1:8444", uuid: "8b9a2f4a-5e51-47a6-b012-75c9dfe8bc30", password: "secret" }
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
      if (!["hysteria2", "tuic"].includes(testCase.protocol)) {
        expect(settings.server, `${testCase.protocol} server leak`).toBeUndefined();
      }
      if (testCase.protocol !== "tuic") {
        expect(settings.uuid, `${testCase.protocol} uuid leak`).toBeUndefined();
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

  it("serializes freedom IP strategy and IPv6 literal guard", () => {
    const built = buildOutboundInput(
      syncOutboundAfterStructuredChange({
        ...createOutboundEditorState({
          id: 1,
          tag: "freedom",
          protocol: "freedom",
          enabled: true,
          settings: JSON.stringify({
            denyLoopback: true,
            customSetting: "keep-me"
          }),
          streamSettings: "",
          createdAt: "",
          updatedAt: ""
        }),
        freedomIpStrategy: "PreferIPv4",
        freedomRejectIpv6Literal: true
      })
    );

    expect(parseObject(built.settings)).toEqual({
      denyLoopback: true,
      customSetting: "keep-me",
      domainStrategy: "PreferIPv4",
      rejectIpv6Literal: true
    });

    expect(
      createOutboundEditorState({
        id: 1,
        tag: "freedom",
        protocol: "freedom",
        enabled: true,
        settings: JSON.stringify({ ip_strategy: "prefer-ipv6" }),
        streamSettings: "",
        createdAt: "",
        updatedAt: ""
      }).freedomIpStrategy
    ).toBe("PreferIPv6");

    const cleared = buildOutboundInput(
      syncOutboundAfterStructuredChange({
        ...createOutboundEditorState({
          id: 1,
          tag: "freedom",
          protocol: "freedom",
          enabled: true,
          settings: JSON.stringify({
            domainStrategy: "UseIPv4",
            rejectIpv6Literal: true
          }),
          streamSettings: "",
          createdAt: "",
          updatedAt: ""
        }),
        freedomIpStrategy: "auto",
        freedomRejectIpv6Literal: false
      })
    );

    expect(parseObject(cleared.settings)).toEqual({});
  });

  it("round-trips hysteria2 settings without leaking unrelated structured fields", () => {
    const defaultState = createOutboundEditorState(null);
    expect(defaultState.hysteria2EndpointShards).toBe("1");
    expect(defaultState.hysteria2CongestionMode).toBe("standard");
    expect(defaultState.hysteria2MinAckRate).toBe("0.8");
    expect(defaultState.hysteria2QuicEndpoints).toBe("1");
    expect(defaultState.hysteria2DatagramPolicy).toBe("standard");
    expect(defaultState.hysteria2FecMode).toBe("off");

    const outbound: Outbound = {
      id: 2,
      tag: "hy2-main",
      protocol: "hysteria2",
      enabled: true,
      settings: JSON.stringify({
        server: "127.0.0.1:443",
        auth: "shared-secret",
        serverName: "old.example.com",
        skipCertVerify: true,
        endpointShards: 2,
        congestion: {
          mode: "brutal-compatible",
          minAckRate: 0.75
        },
        quic: {
          reusePort: true,
          recvBufferBytes: 16777216
        },
        datagram: {
          enabled: true,
          policy: "h2-plus"
        },
        fec: {
          mode: "auto",
          maxOverheadPercent: 20
        },
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
      hysteria2Auth: "next-secret",
      hysteria2ServerName: "hy2.example.com",
      hysteria2SkipCertVerify: false,
      hysteria2EndpointShards: "4",
      hysteria2CongestionMode: "badnet-throughput",
      hysteria2MinAckRate: "0.6",
      hysteria2MaxQueueDelayMs: "100",
      hysteria2PacingGain: "1.5",
      hysteria2LossCompensation: false,
      hysteria2QuicReusePort: false,
      hysteria2QuicEndpoints: "cpu",
      hysteria2QuicRecvBufferBytes: "33554432",
      hysteria2QuicSendBufferBytes: "16777216",
      hysteria2DatagramEnabled: true,
      hysteria2DatagramUdpOverDatagram: false,
      hysteria2DatagramPolicy: "standard",
      hysteria2FecMode: "xor1-of-n",
      hysteria2FecMaxOverheadPercent: "15",
      tlsServerName: "new.example.com",
      address: "127.0.0.9",
      port: "9000"
    });
    const built = buildOutboundInput(state);
    const settings = parseObject(built.settings);
    const streamSettings = parseObject(built.streamSettings);

    expect(settings.server).toBe("127.0.0.2:8443");
    expect(settings.auth).toBe("next-secret");
    expect(settings.serverName).toBe("hy2.example.com");
    expect(settings.skipCertVerify).toBeUndefined();
    expect(settings.endpointShards).toBe(4);
    expect(settings.congestion).toMatchObject({
      mode: "badnet-throughput",
      minAckRate: 0.6,
      maxQueueDelayMs: 100,
      pacingGain: 1.5,
      lossCompensation: false
    });
    expect(settings.quic).toMatchObject({
      endpoints: "cpu",
      recvBufferBytes: 33554432,
      sendBufferBytes: 16777216
    });
    expect(settings.quic.reusePort).toBe(false);
    expect(settings.datagram).toMatchObject({
      enabled: true,
      udpOverDatagram: false
    });
    expect(settings.datagram.policy).toBe("standard");
    expect(settings.fec).toMatchObject({
      mode: "xor1-of-n",
      maxOverheadPercent: 15
    });
    expect(settings.customSetting).toBe("keep-me");
    expect(settings.address).toBeUndefined();
    expect(settings.port).toBeUndefined();
    expect(streamSettings.tlsSettings.serverName).toBe("new.example.com");
    expect(streamSettings.tlsSettings.customTls).toBe("keep-tls");
  });

  it("serializes simple hysteria2 performance modes like the save button output", () => {
    const common = {
      ...createOutboundEditorState(),
      protocol: "hysteria2",
      server: "203.0.113.10:443",
      hysteria2Auth: "shared-secret",
      hysteria2ServerName: "hy2.example.com"
    };

    const balanced = parseObject(
      buildOutboundInput(
        syncOutboundAfterStructuredChange({
          ...common,
          hysteria2CongestionMode: "standard"
        })
      ).settings
    );
    expect(balanced).toEqual({
      server: "203.0.113.10:443",
      auth: "shared-secret",
      serverName: "hy2.example.com"
    });

    const throughput = parseObject(
      buildOutboundInput(
        syncOutboundAfterStructuredChange({
          ...common,
          hysteria2CongestionMode: "brutal-compatible"
        })
      ).settings
    );
    expect(throughput).toEqual({
      server: "203.0.113.10:443",
      auth: "shared-secret",
      serverName: "hy2.example.com",
      congestion: { mode: "brutal-compatible" }
    });

    const lowLatency = parseObject(
      buildOutboundInput(
        syncOutboundAfterStructuredChange({
          ...common,
          hysteria2CongestionMode: "badnet-low-latency"
        })
      ).settings
    );
    expect(lowLatency).toEqual({
      server: "203.0.113.10:443",
      auth: "shared-secret",
      serverName: "hy2.example.com",
      congestion: { mode: "badnet-low-latency" }
    });
  });

  it("persists explicit default-looking hysteria2 outbound overrides", () => {
    const state = syncOutboundAfterStructuredChange({
      ...createOutboundEditorState(),
      protocol: "hysteria2",
      server: "203.0.113.10:443",
      hysteria2Auth: "shared-secret",
      hysteria2TransportOverrides: true
    });
    const settings = parseObject(buildOutboundInput(state).settings);

    expect(settings.endpointShards).toBe(1);
    expect(settings.quic).toEqual({
      reusePort: false,
      endpoints: 1,
      recvBufferBytes: 8388608,
      sendBufferBytes: 8388608
    });
    expect(settings.datagram).toEqual({
      enabled: false,
      udpOverDatagram: true,
      policy: "standard"
    });
    expect(settings.fec).toEqual({ mode: "off" });
    expect(state.hysteria2TransportOverrides).toBe(true);
  });

  it("covers the structured outbound transport and security matrix", () => {
    const base = {
      ...createOutboundEditorState(),
      protocol: "vless",
      address: "127.0.0.1",
      port: "443",
      userId: "459dc0c8-d891-4768-9234-faf11fd26b5d"
    };

    const networks = [
      { network: "tcp", settings: {}, extras: {} },
      { network: "ws", settings: { wsSettings: { path: "/ws", headers: { Host: "ws.example.com" } } }, extras: { wsPath: "/ws", wsHost: "ws.example.com" } },
      { network: "grpc", settings: { grpcSettings: { serviceName: "GunService" } }, extras: { grpcServiceName: "GunService" } },
      { network: "httpupgrade", settings: { httpupgradeSettings: { path: "/upgrade", host: "edge.example.com" } }, extras: { httpupgradePath: "/upgrade", httpupgradeHost: "edge.example.com" } },
      { network: "splithttp", settings: { splithttpSettings: { path: "/packet" } }, extras: { splitHttpPath: "/packet" } },
      { network: "quic", settings: {}, extras: {} }
    ] as const;

    const securities = [
      { security: "none", patch: {}, settings: {} },
      { security: "tls", patch: { tlsServerName: "tls.example.com", tlsAllowInsecure: true }, settings: { tlsSettings: { serverName: "tls.example.com", allowInsecure: true } } },
      {
        security: "reality",
        patch: {
          realityServerName: "www.microsoft.com",
          realityPublicKey: "e1df9c8812b5ce9b3bd36da542896be856ad0a6c6e6df9d910a4040c07268142",
          realityShortId: "feedbeef"
        },
        settings: {
          realitySettings: { serverName: "www.microsoft.com", shortId: "feedbeef", shortIds: ["feedbeef"], publicKey: "e1df9c8812b5ce9b3bd36da542896be856ad0a6c6e6df9d910a4040c07268142" }
        }
      }
    ] as const;

    for (const network of networks) {
      for (const security of securities) {
        const label = `${network.network}-${security.security}`;
        const built = buildOutboundInput(
          syncOutboundAfterStructuredChange({
            ...base,
            ...network.extras,
            ...security.patch,
            network: network.network,
            security: security.security
          })
        );
        const streamSettings = parseObject(built.streamSettings);

        expect(streamSettings, label).toMatchObject({
          network: network.network,
          security: security.security,
          ...network.settings,
          ...security.settings
        });

        if (security.security === "none") {
          expect(streamSettings.tlsSettings).toBeUndefined();
          expect(streamSettings.realitySettings).toBeUndefined();
        } else if (security.security === "tls") {
          expect(streamSettings.realitySettings).toBeUndefined();
        } else {
          expect(streamSettings.tlsSettings).toBeUndefined();
        }
      }
    }
  });

  it("keeps enabled empty SplitHTTP option groups through structured sync", () => {
    const initial = createOutboundEditorState();
    const state = syncOutboundAfterStructuredChange({
      ...initial,
      network: "splithttp",
      splitHttp: { ...initial.splitHttp, xmuxEnabled: true, downloadEnabled: true }
    });
    expect(state.splitHttp.xmuxEnabled).toBe(true);
    expect(state.splitHttp.downloadEnabled).toBe(true);
    expect(parseObject(buildOutboundInput(state).streamSettings).splithttpSettings).toMatchObject({ xmux: {}, downloadSettings: {} });
  });
});
