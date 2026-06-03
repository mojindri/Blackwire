import { describe, expect, it } from "vitest";
import {
  activeUserProtocol,
  buildUserInput,
  createUserEditorState,
  replaceCredentialJson,
  syncCredentialFromFields,
  validateUserState
} from "./userConfigurator";
import type { Inbound, ManagedUser } from "./types";

const inbounds: Inbound[] = [
  {
    id: 1,
    tag: "vless-main",
    listen: "0.0.0.0",
    port: 443,
    protocol: "vless",
    enabled: true,
    transport: "tcp",
    settings: "{}",
    streamSettings: "{}",
    sniffing: "{}",
    limits: "{}",
    createdAt: "",
    updatedAt: ""
  },
  {
    id: 2,
    tag: "trojan-main",
    listen: "0.0.0.0",
    port: 8443,
    protocol: "trojan",
    enabled: true,
    transport: "tcp",
    settings: "{}",
    streamSettings: "{}",
    sniffing: "{}",
    limits: "{}",
    createdAt: "",
    updatedAt: ""
  },
  {
    id: 3,
    tag: "hy2-main",
    listen: "0.0.0.0",
    port: 8444,
    protocol: "hysteria2",
    enabled: true,
    transport: "tcp",
    settings: "{}",
    streamSettings: "{}",
    sniffing: "{}",
    limits: "{}",
    createdAt: "",
    updatedAt: ""
  }
];

describe("userConfigurator", () => {
  it("preserves unknown credential keys while applying structured password edits", () => {
    const user: ManagedUser = {
      id: 10,
      inboundId: 2,
      email: "alice@example.com",
      uuid: "459dc0c8-d891-4768-9234-faf11fd26b5d",
      flow: "",
      credential: { password: "old-secret", customKey: "keep-me" },
      note: "",
      enabled: true,
      trafficLimitBytes: null,
      expiryAt: null,
      uploadBytes: 0,
      downloadBytes: 0,
      subToken: "token",
      enforcementStatus: "active",
      createdAt: "",
      updatedAt: ""
    };

    const protocol = activeUserProtocol(inbounds, user.inboundId);
    const state = syncCredentialFromFields(
      {
        ...createUserEditorState(user, inbounds),
        password: "new-secret",
        trafficLimitGB: "12.5"
      },
      protocol
    );
    const built = buildUserInput(state, protocol);

    expect(built.credential?.password).toBe("new-secret");
    expect(built.credential?.customKey).toBe("keep-me");
    expect(built.trafficLimitBytes).toBe(Math.round(12.5 * 1024 * 1024 * 1024));
  });

  it("drops irrelevant owned keys when the inbound protocol changes", () => {
    const trojanState = syncCredentialFromFields(
      {
        ...createUserEditorState(null, inbounds),
        inboundId: 2,
        uuid: "459dc0c8-d891-4768-9234-faf11fd26b5d",
        password: "shared-secret",
        credentialText: JSON.stringify({ password: "shared-secret", customKey: "keep-me" }, null, 2)
      },
      "trojan"
    );

    const switched = syncCredentialFromFields(
      {
        ...trojanState,
        inboundId: 1,
        flow: "xtls-rprx-vision"
      },
      "vless"
    );
    const built = buildUserInput(switched, "vless");

    expect(built.flow).toBe("xtls-rprx-vision");
    expect(built.credential?.password).toBeUndefined();
    expect(built.credential?.customKey).toBe("keep-me");
  });

  it("keeps invalid advanced JSON text instead of stomping it during structured edits", () => {
    const invalid = replaceCredentialJson(createUserEditorState(null, inbounds), "{invalid");
    const next = syncCredentialFromFields({ ...invalid, password: "secret" }, "trojan");

    expect(next.credentialText).toBe("{invalid");
    expect(next.credentialError).not.toBe("");
  });

  it("validates protocol-specific access requirements", () => {
    const trojanIssues = validateUserState(
      {
        ...createUserEditorState(null, inbounds),
        inboundId: 2,
        email: "alice@example.com",
        uuid: "not-a-uuid",
        password: "",
        trafficLimitGB: "0"
      },
      "trojan",
      inbounds
    );
    const hy2Issues = validateUserState(
      {
        ...createUserEditorState(null, inbounds),
        inboundId: 3,
        email: "alice@example.com",
        uuid: "459dc0c8-d891-4768-9234-faf11fd26b5d",
        auth: ""
      },
      "hysteria2",
      inbounds
    );

    expect(trojanIssues.map((issue) => issue.field)).toEqual(["uuid", "password", "trafficLimitGB"]);
    expect(hy2Issues.map((issue) => issue.field)).toContain("auth");
  });
});
