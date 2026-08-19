import { AlertCircle, KeyRound, Save, Terminal, Trash2, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "../../lib/api";
import { defaultHysteria2CongestionTuning, defaultHysteria2TransportTuning, hasCustomHysteria2Congestion, HYSTERIA2_SIMPLE_CONGESTION_MODES } from "../../lib/hysteria2Tuning";
import type { CapabilityMap, Inbound, InboundInput } from "../../lib/types";
import {
  buildInboundInput,
  createInboundEditorState,
  inboundCompatibilityNotice,
  inboundSummary,
  syncAfterStructuredChange,
  validateInboundState,
  type InboundEditorState
} from "../../lib/inboundConfigurator";
import {
  buildTlsSelfSignedInput,
  defaultTlsSelfSignedValues,
  expectedTlsSelfSignedPaths,
  type TlsSelfSignedValues
} from "../../lib/tlsSelfSigned";
import { Button } from "../atoms/Button";
import { IconButton } from "../atoms/IconButton";
import { Input, Select } from "../atoms/Input";
import { Switch } from "../atoms/Switch";
import { Field } from "../molecules/Field";
import { SplitHttpFields } from "./SplitHttpFields";
import { Hysteria2TuningFields } from "./Hysteria2TuningFields";

type TabKey = "basic" | "protocol" | "transport" | "security" | "sniffing";

const sniffingOptions = ["http", "tls", "fakedns"];
const tabOrder: Array<{ key: TabKey; label: string }> = [
  { key: "basic", label: "Basic" },
  { key: "protocol", label: "Protocol" },
  { key: "transport", label: "Transport" },
  { key: "security", label: "Security" },
  { key: "sniffing", label: "Sniffing & limits" }
];

export function InboundDrawer({
  editing,
  inboundsCount,
  capabilities,
  busy,
  onClose,
  onCreate,
  onUpdate,
  onDelete
}: {
  editing: Inbound | null;
  inboundsCount: number;
  capabilities: CapabilityMap | null;
  busy: boolean;
  onClose: () => void;
  onCreate: (input: InboundInput) => void;
  onUpdate: (id: number, input: InboundInput) => void;
  onDelete: (id: number) => void;
}) {
  const [activeTab, setActiveTab] = useState<TabKey>("basic");
  const [state, setState] = useState<InboundEditorState>(() => createInboundEditorState(editing));
  const stateRef = useRef(state);
  const [realityImportMessage, setRealityImportMessage] = useState("");
  const [realityImportBusy, setRealityImportBusy] = useState(false);
  const [realityGenerateBusy, setRealityGenerateBusy] = useState(false);
  const [tlsSelfSignedOpen, setTlsSelfSignedOpen] = useState(false);
  const [tlsSelfSigned, setTlsSelfSigned] = useState<TlsSelfSignedValues>(() => defaultTlsSelfSignedValues(""));
  const [tlsSelfSignedBusy, setTlsSelfSignedBusy] = useState(false);
  const [tlsSelfSignedMessage, setTlsSelfSignedMessage] = useState("");

  useEffect(() => {
    const next = createInboundEditorState(editing);
    stateRef.current = next;
    setState(next);
    setRealityImportMessage("");
    setRealityGenerateBusy(false);
    setTlsSelfSignedOpen(false);
    setTlsSelfSignedBusy(false);
    setTlsSelfSignedMessage("");
    setActiveTab("basic");
  }, [editing]);

  const protocolOptions = useMemo(
    () =>
      capabilities?.protocols.filter((item) =>
        ["vless", "vmess", "trojan", "shadowsocks", "hysteria2", "tuic", "socks", "http"].includes(item.key)
      ) ?? [
        { key: "vless", label: "VLESS", status: "supported", notes: "" },
        { key: "vmess", label: "VMess", status: "supported", notes: "" },
        { key: "trojan", label: "Trojan", status: "supported", notes: "" },
        { key: "shadowsocks", label: "Shadowsocks", status: "supported", notes: "" },
        { key: "hysteria2", label: "Hysteria2", status: "supported", notes: "" },
        { key: "tuic", label: "TUIC v5", status: "supported", notes: "QUIC v5 TCP and UDP" },
        { key: "socks", label: "SOCKS5", status: "supported", notes: "" },
        { key: "http", label: "HTTP CONNECT", status: "supported", notes: "" }
      ],
    [capabilities]
  );
  const transportOptions = useMemo(
    () => {
      const visible =
        capabilities?.transports.filter((item) =>
          ["tcp", "ws", "grpc", "httpupgrade", "splithttp", "quic"].includes(item.key)
        ) ?? [
          { key: "tcp", label: "TCP", status: "supported", notes: "" },
          { key: "ws", label: "WebSocket", status: "supported", notes: "" },
          { key: "grpc", label: "gRPC", status: "supported", notes: "" },
          { key: "httpupgrade", label: "HTTPUpgrade", status: "supported", notes: "" },
          { key: "splithttp", label: "SplitHTTP", status: "supported", notes: "" },
          { key: "quic", label: "QUIC", status: "supported", notes: "" }
        ];
      const current = capabilities?.transports.find((item) => item.key === state.network);
      if (current && !visible.some((item) => item.key === current.key)) {
        return [...visible, current];
      }
      if (!current && state.network && !visible.some((item) => item.key === state.network)) {
        return [...visible, { key: state.network, label: state.network, status: "deprecated", notes: "Legacy transport retained for editing existing configs" }];
      }
      return visible;
    },
    [capabilities, state.network]
  );
  const securityOptions = useMemo(
    () =>
      capabilities?.security.filter((item) => ["none", "tls", "reality", "shadowtls"].includes(item.key)) ?? [
        { key: "none", label: "No security", status: "supported", notes: "" },
        { key: "tls", label: "TLS", status: "supported", notes: "" },
        { key: "reality", label: "REALITY", status: "supported", notes: "" },
        { key: "shadowtls", label: "ShadowTLS v3", status: "supported", notes: "" }
      ],
    [capabilities]
  );

  const currentSummary = editing ? inboundSummary(editing) : { network: state.network, security: state.security, detail: "" };
  const compatibilityNotice = inboundCompatibilityNotice(state);
  const jsonErrors = [state.settings, state.streamSettings, state.sniffing, state.limits].filter((slice) => slice.error);
  const validationIssues = validateInboundState(state);
  const canDelete = !busy && inboundsCount > 1;
  const saveDisabled = busy || jsonErrors.length > 0 || validationIssues.length > 0;
  const tlsSelfSignedPreview = useMemo(() => expectedTlsSelfSignedPaths(tlsSelfSigned.serverName), [tlsSelfSigned.serverName]);
  const hysteria2CustomCongestion = state.protocol === "hysteria2" && hasCustomHysteria2Congestion(state);
  const hysteria2TransportOverrides = state.protocol === "hysteria2" && state.hysteria2TransportOverrides;

  const updateStructured = (patch: Partial<InboundEditorState>) => {
    const next = syncAfterStructuredChange({ ...stateRef.current, ...patch });
    stateRef.current = next;
    setState(next);
  };

  const updateHysteria2PerformanceMode = (value: string) => {
    if (value === "custom") {
      const current = stateRef.current;
      updateStructured({
        hysteria2CongestionMode: HYSTERIA2_SIMPLE_CONGESTION_MODES.has(current.hysteria2CongestionMode)
          ? "badnet-throughput"
          : current.hysteria2CongestionMode
      });
      return;
    }
    updateStructured(defaultHysteria2CongestionTuning(value));
  };

  const submit = () => {
    const latest = stateRef.current;
    const latestJsonErrors = [latest.settings, latest.streamSettings, latest.sniffing, latest.limits].filter((slice) => slice.error);
    if (busy || latestJsonErrors.length > 0 || validateInboundState(latest).length > 0) return;
    const input = buildInboundInput(latest);
    if (editing) {
      onUpdate(editing.id, input);
    } else {
      onCreate(input);
    }
    onClose();
  };

  const importRealityValues = async () => {
    setRealityImportBusy(true);
    setRealityImportMessage("");
    try {
      const values = await api.realityClientValues();
      const current = stateRef.current;
      const selected = values.find((item) => item.tag === current.tag) ?? values[0];
      if (!selected) {
        setRealityImportMessage("No server-generated REALITY values found.");
        return;
      }
      updateStructured({
        realityPrivateKey: selected.privateKey ?? current.realityPrivateKey,
        realityPublicKey: selected.publicKey,
        realityShortId: selected.shortId,
        realityServerName: selected.serverName,
        realityDest: selected.dest ?? current.realityDest,
        security: "reality",
        network: "tcp"
      });
      const source = selected.tag ? `${selected.source} (${selected.tag})` : selected.source;
      setRealityImportMessage(`Loaded REALITY values from ${source}.`);
    } catch (error) {
      setRealityImportMessage(error instanceof Error ? error.message : "Failed to load server REALITY values.");
    } finally {
      setRealityImportBusy(false);
    }
  };

  const generateRealityValues = async () => {
    setRealityGenerateBusy(true);
    setRealityImportMessage("");
    try {
      const generated = await api.realityGenerateValues();
      updateStructured({
        realityPrivateKey: generated.privateKey,
        realityPublicKey: generated.publicKey,
        realityShortId: generated.shortId,
        security: "reality",
        network: "tcp"
      });
      setRealityImportMessage("Generated matching REALITY private key, public key, and short ID.");
    } catch (error) {
      setRealityImportMessage(error instanceof Error ? error.message : "Failed to generate REALITY values.");
    } finally {
      setRealityGenerateBusy(false);
    }
  };

  const openTlsSelfSignedDialog = () => {
    setTlsSelfSigned(defaultTlsSelfSignedValues(stateRef.current.tlsServerName));
    setTlsSelfSignedMessage("");
    setTlsSelfSignedOpen(true);
  };

  const updateTlsSelfSigned = (patch: Partial<TlsSelfSignedValues>) => {
    setTlsSelfSigned((current) => ({ ...current, ...patch }));
    setTlsSelfSignedMessage("");
  };

  const generateTlsSelfSignedCertificate = async () => {
    setTlsSelfSignedBusy(true);
    setTlsSelfSignedMessage("");
    try {
      const generated = await api.tlsGenerateSelfSigned(buildTlsSelfSignedInput(tlsSelfSigned));
      updateStructured({
        tlsServerName: generated.serverName,
        tlsCertificateFile: generated.certificateFile,
        tlsKeyFile: generated.keyFile
      });
      setTlsSelfSignedOpen(false);
      setTlsSelfSignedMessage("Generated certificate on the server and applied the paths.");
    } catch (error) {
      setTlsSelfSignedMessage(error instanceof Error ? error.message : "Failed to generate certificate.");
    } finally {
      setTlsSelfSignedBusy(false);
    }
  };

  return (
    <aside className="drawer drawer-wide">
      <div className="drawer-head">
        <div>
          <h2>{editing ? editing.tag : "New inbound"}</h2>
          <p>
            {editing
              ? "Structured inbound configuration with protocol-aware typed fields."
              : "Create a new inbound with guided protocol, transport, security, and sniffing settings."}
          </p>
        </div>
        <IconButton label="Close" onClick={onClose}>
          <X size={18} />
        </IconButton>
      </div>
      <div className="drawer-body drawer-body-configurator">
        <section className="drawer-card drawer-summary-card">
          <div className="summary-head">
            <div>
              <strong>{state.tag || "Untitled inbound"}</strong>
              <span>
                {state.listen}:{state.port}
              </span>
            </div>
            <Switch checked={state.enabled} onChange={(enabled) => updateStructured({ enabled })} label={state.enabled ? "Enabled" : "Disabled"} />
          </div>
          <div className="summary-badges">
            <span className="summary-chip">{state.protocol}</span>
            <span className="summary-chip">{state.network}</span>
            <span className="summary-chip">{state.security}</span>
            {currentSummary.detail ? <span className="summary-chip summary-chip-soft">{currentSummary.detail}</span> : null}
          </div>
        </section>

        <div className="configurator-tabs" role="tablist" aria-label="Inbound editor sections">
          {tabOrder.map((tab) => (
            <button
              key={tab.key}
              type="button"
              className={`configurator-tab ${activeTab === tab.key ? "configurator-tab-active" : ""}`}
              onClick={() => setActiveTab(tab.key)}
            >
              {tab.label}
            </button>
          ))}
        </div>

        {activeTab === "basic" ? (
          <section className="drawer-card configurator-section">
            <div className="configurator-grid">
              <Field label="Tag">
                <Input value={state.tag} onChange={(e) => updateStructured({ tag: e.target.value })} />
              </Field>
              <Field label="Protocol">
                <Select value={state.protocol} onChange={(e) => updateStructured({ protocol: e.target.value })}>
                  {protocolOptions.map((item) => (
                    <option key={item.key} value={item.key} disabled={item.status === "unsupported"} title={item.notes}>
                      {capabilityLabel(item)}
                    </option>
                  ))}
                </Select>
              </Field>
              <Field label="Listen host">
                <Input value={state.listen} onChange={(e) => updateStructured({ listen: e.target.value })} />
              </Field>
              <Field label="Port">
                <Input
                  type="number"
                  min={1}
                  max={65535}
                  value={state.port}
                  onChange={(e) => updateStructured({ port: Number(e.target.value) || 0 })}
                />
              </Field>
            </div>
          </section>
        ) : null}

        {activeTab === "protocol" ? (
          <section className="drawer-card configurator-section">
            {state.protocol === "vless" ? (
              <Field label="Decryption">
                <Input
                  value={state.decryption}
                  onChange={(e) => updateStructured({ decryption: e.target.value })}
                  placeholder="none"
                />
              </Field>
            ) : null}
            {state.protocol === "vmess" ? (
              <p className="field-hint">VMess inbound accepts AEAD body security only. Clients must not use VMess body security none.</p>
            ) : null}
            {state.protocol === "shadowsocks" ? (
              <Field label="Method" hint="Inbound-level method only. Managed users still live in Users.">
                <Input value={state.shadowsocksMethod} onChange={(e) => updateStructured({ shadowsocksMethod: e.target.value })} placeholder="2022-blake3-aes-128-gcm" />
              </Field>
            ) : null}
            {state.protocol === "trojan" ? (
              <p className="field-hint">Trojan client secrets are managed through Users.</p>
            ) : null}
            {state.protocol === "hysteria2" ? (
              <>
                <p className="field-hint">Hysteria2 is UDP/QUIC-based. Start with a performance mode; Custom exposes the lower-level tuning fields.</p>
                <div className="configurator-grid">
                  <Field label="Auth" hint="Server-side shared auth secret. Managed users can also provide per-user auth values.">
                    <Input value={state.hysteria2Auth} onChange={(e) => updateStructured({ hysteria2Auth: e.target.value })} placeholder="shared-secret" />
                  </Field>
                  <Field label="Performance mode" hint="Throughput restores the aggressive Hysteria2-style behavior; Balanced is conservative.">
                    <Select
                      value={hysteria2CustomCongestion ? "custom" : state.hysteria2CongestionMode}
                      onChange={(e) => updateHysteria2PerformanceMode(e.target.value)}
                    >
                      <option value="standard">Balanced</option>
                      <option value="brutal-compatible">Throughput</option>
                      <option value="badnet-low-latency">Low latency</option>
                      <option value="custom">Custom</option>
                    </Select>
                  </Field>
                </div>
                {hysteria2CustomCongestion ? (
                  <div className="configurator-grid">
                    <Field label="Congestion mode">
                      <Select value={state.hysteria2CongestionMode} onChange={(e) => updateStructured({ hysteria2CongestionMode: e.target.value })}>
                        <option value="standard">standard</option>
                        <option value="brutal-compatible">brutal-compatible</option>
                        <option value="badnet-throughput">badnet-throughput</option>
                        <option value="badnet-low-latency">badnet-low-latency</option>
                        <option value="nova-cc">nova-cc</option>
                        <option value="auto-probe">auto-probe</option>
                      </Select>
                    </Field>
                  <Field label="Min ACK rate" hint="Optional congestion clamp, for example 0.8.">
                    <Input value={state.hysteria2MinAckRate} onChange={(e) => updateStructured({ hysteria2MinAckRate: e.target.value })} placeholder="0.8" />
                  </Field>
                  <Field label="Max queue delay ms">
                    <Input value={state.hysteria2MaxQueueDelayMs} onChange={(e) => updateStructured({ hysteria2MaxQueueDelayMs: e.target.value })} placeholder="80" />
                  </Field>
                  <Field label="Pacing gain">
                    <Input value={state.hysteria2PacingGain} onChange={(e) => updateStructured({ hysteria2PacingGain: e.target.value })} placeholder="1.25" />
                  </Field>
                  <Switch checked={state.hysteria2LossCompensation} onChange={(hysteria2LossCompensation) => updateStructured({ hysteria2LossCompensation })} label="Loss compensation" />
                  </div>
                ) : null}
              </>
            ) : null}
            {state.protocol === "socks" || state.protocol === "http" ? (
              <p className="field-hint">Listener basics are managed here. Add managed credentials through Users when authentication is required.</p>
            ) : null}
            {!["vless", "vmess", "shadowsocks", "trojan", "hysteria2", "socks", "http"].includes(state.protocol) ? (
              <p className="field-hint">This legacy protocol has no editable protocol-specific fields in this panel; its existing stored values are preserved.</p>
            ) : null}
          </section>
        ) : null}

        {activeTab === "transport" ? (
          <section className="drawer-card configurator-section">
            <div className="configurator-grid">
              <Field label="Network">
                <Select value={state.network} onChange={(e) => updateStructured({ network: e.target.value })}>
                  {transportOptions.map((item) => (
                    <option key={item.key} value={item.key} disabled={item.status === "unsupported"} title={item.notes}>
                      {capabilityLabel(item)}
                    </option>
                  ))}
                </Select>
              </Field>
            </div>

            {state.network === "ws" ? (
              <div className="configurator-grid">
                <Field label="Path">
                  <Input value={state.wsPath} onChange={(e) => updateStructured({ wsPath: e.target.value })} placeholder="/vless-main" />
                </Field>
                <Field label="Host header">
                  <Input value={state.wsHost} onChange={(e) => updateStructured({ wsHost: e.target.value })} placeholder="edge.example.com" />
                </Field>
              </div>
            ) : null}

            {state.network === "grpc" ? (
              <Field label="Service name">
                <Input value={state.grpcServiceName} onChange={(e) => updateStructured({ grpcServiceName: e.target.value })} placeholder="GunService" />
              </Field>
            ) : null}

            {state.network === "httpupgrade" ? (
              <div className="configurator-grid">
                <Field label="Path">
                  <Input value={state.httpupgradePath} onChange={(e) => updateStructured({ httpupgradePath: e.target.value })} placeholder="/upgrade" />
                </Field>
                <Field label="Host">
                  <Input value={state.httpupgradeHost} onChange={(e) => updateStructured({ httpupgradeHost: e.target.value })} placeholder="edge.example.com" />
                </Field>
              </div>
            ) : null}

            {state.network === "splithttp" ? (
              <SplitHttpFields value={state.splitHttp} onChange={(splitHttp) => updateStructured({ splitHttp })} />
            ) : null}

            {state.network === "quic" ? (
              <p className="field-hint">QUIC transport uses the runtime defaults. Hysteria2 exposes its additional socket tuning as typed controls below.</p>
            ) : null}

            {state.protocol === "hysteria2" ? <>
              <Switch checked={hysteria2TransportOverrides} onChange={(enabled) => {
                updateStructured({
                  hysteria2TransportOverrides: enabled,
                  ...(!enabled ? defaultHysteria2TransportTuning() : {})
                });
              }} label="Override Hysteria2 transport defaults" />
              <p className="field-hint">Off inherits Blackwire's global QUIC, datagram, and FEC behavior.</p>
            </> : null}
            {hysteria2TransportOverrides ? <Hysteria2TuningFields direction="inbound" value={state} onChange={updateStructured} /> : null}
          </section>
        ) : null}

        {activeTab === "security" ? (
          <section className="drawer-card configurator-section">
            <Field label="Security layer">
              <Select value={state.security} onChange={(e) => updateStructured({ security: e.target.value })}>
                {securityOptions.map((item) => (
                  <option key={item.key} value={item.key}>
                    {item.label}
                  </option>
                ))}
              </Select>
            </Field>

            {state.security === "tls" ? (
              <>
                <p className="field-hint">
                  Configure TLS explicitly. The server name must match the certificate clients will validate.
                </p>
                <div className="configurator-helper-row">
                  <div>
                    <strong>Self-signed certificate</strong>
                    <span>Generate server-side certificate files when you need IP-only or lab TLS.</span>
                  </div>
                  <Button type="button" variant="secondary" icon={<Terminal size={16} />} onClick={openTlsSelfSignedDialog}>
                    Self-Signed Helper
                  </Button>
                </div>
                {!tlsSelfSignedOpen && tlsSelfSignedMessage ? <p className="copy-feedback">{tlsSelfSignedMessage}</p> : null}
                <div className="configurator-grid">
                  <Field label="Server name" hint="Used as SNI in generated client links.">
                    <Input value={state.tlsServerName} onChange={(e) => updateStructured({ tlsServerName: e.target.value })} placeholder="example.com" />
                  </Field>
                  <Field label="ALPN" hint="Comma-separated list, for example h2, http/1.1.">
                    <Input value={state.tlsAlpn} onChange={(e) => updateStructured({ tlsAlpn: e.target.value })} placeholder="h2, http/1.1" />
                  </Field>
                  <Field label="Certificate file" hint="Server-side certificate path. Pair it with the matching key file.">
                    <Input value={state.tlsCertificateFile} onChange={(e) => updateStructured({ tlsCertificateFile: e.target.value })} placeholder="/etc/blackwire/fullchain.pem" />
                  </Field>
                  <Field label="Key file" hint="Server-side private key path for the certificate.">
                    <Input value={state.tlsKeyFile} onChange={(e) => updateStructured({ tlsKeyFile: e.target.value })} placeholder="/etc/blackwire/privkey.pem" />
                  </Field>
                </div>
                <p className="field-hint">
                  Domain setup: point the domain to this server, issue a trusted certificate for that exact name, set Server name to the domain, then use the certificate and key paths.
                </p>
                <p className="field-hint">
                  Public-IP or self-signed setup: set Server name to the value your client will use, provide the self-signed certificate and key, and expect clients to require insecure/skip-cert-verify mode.
                </p>
              </>
            ) : null}

            {state.security === "reality" ? (
              <>
                <p className="field-hint">
                  Use the values generated by the running server. The public key is derived from the server private key, and the short ID must match one of the server shortIds entries.
                </p>
                <p className="field-hint">
                  Cover safety: choose a stable, directly reachable TLS 1.3 site whose handshake resembles normal traffic in your region. Avoid Microsoft, Apple/iCloud, and targets in heavily filtered country domains; these choices can increase blocking risk.
                </p>
                <Button type="button" variant="secondary" icon={<KeyRound size={16} />} onClick={generateRealityValues} loading={realityGenerateBusy} disabled={busy || realityGenerateBusy}>
                  Generate REALITY Key Pair
                </Button>
                <Button type="button" variant="secondary" icon={<KeyRound size={16} />} onClick={importRealityValues} loading={realityImportBusy} disabled={busy || realityImportBusy}>
                  Load Server REALITY Values
                </Button>
                {realityImportMessage ? <p className="field-hint">{realityImportMessage}</p> : null}
                <div className="configurator-grid">
                  <Field label="Server name" hint="Must be allowed by server realitySettings.serverNames.">
                    <Input value={state.realityServerName} onChange={(e) => updateStructured({ realityServerName: e.target.value })} placeholder="www.cloudflare.com" />
                  </Field>
                  <Field label="Private key" hint="Server-side hex, base64url, or standard Base64 key stored in the inbound config. Generated client links use the matching public key.">
                    <Input value={state.realityPrivateKey} onChange={(e) => updateStructured({ realityPrivateKey: e.target.value })} placeholder="32-byte X25519 private key" />
                  </Field>
                  <Field label="Public key" hint="Paste from server client-info or derive from the REALITY privateKey; do not randomize this value.">
                    <Input value={state.realityPublicKey} onChange={(e) => updateStructured({ realityPublicKey: e.target.value })} placeholder="base64-x25519-public-key" />
                  </Field>
                  <Field label="Short ID" hint="Must exactly match one server shortIds value; changing one character breaks REALITY auth.">
                    <Input value={state.realityShortId} onChange={(e) => updateStructured({ realityShortId: e.target.value })} placeholder="6ba85179e30d4fc2" />
                  </Field>
                  <Field label="Fallback destination" hint="Server-side fallback socket address used when REALITY auth fails. Use an IP:port, not a domain.">
                    <Input value={state.realityDest} onChange={(e) => updateStructured({ realityDest: e.target.value })} placeholder="93.184.216.34:443" />
                  </Field>
                  <Field label="Fingerprint">
                    <Input value={state.realityFingerprint} onChange={(e) => updateStructured({ realityFingerprint: e.target.value })} placeholder="chrome" />
                  </Field>
                  <Field label="Spider X">
                    <Input value={state.realitySpiderX} onChange={(e) => updateStructured({ realitySpiderX: e.target.value })} placeholder="/" />
                  </Field>
                  <Field label="Fallback upload starts after" hint="Bytes relayed from an unauthenticated client before pacing begins. Leave all fallback limit fields empty to disable.">
                    <Input type="number" min={0} value={state.realityFallbackUploadAfterBytes} onChange={(e) => updateStructured({ realityFallbackUploadAfterBytes: e.target.value })} placeholder="0" />
                  </Field>
                  <Field label="Fallback upload bytes/sec" hint="Sustained client-to-cover limit. This never affects authenticated proxy traffic.">
                    <Input type="number" min={0} value={state.realityFallbackUploadBytesPerSec} onChange={(e) => updateStructured({ realityFallbackUploadBytesPerSec: e.target.value })} placeholder="disabled" />
                  </Field>
                  <Field label="Fallback upload burst" hint="Additional bytes allowed immediately before sustained pacing.">
                    <Input type="number" min={0} value={state.realityFallbackUploadBurstBytesPerSec} onChange={(e) => updateStructured({ realityFallbackUploadBurstBytesPerSec: e.target.value })} placeholder="0" />
                  </Field>
                  <Field label="Fallback download starts after" hint="Bytes returned by the cover before pacing begins.">
                    <Input type="number" min={0} value={state.realityFallbackDownloadAfterBytes} onChange={(e) => updateStructured({ realityFallbackDownloadAfterBytes: e.target.value })} placeholder="0" />
                  </Field>
                  <Field label="Fallback download bytes/sec" hint="Sustained cover-to-client limit. Use only when fallback abuse is a concern.">
                    <Input type="number" min={0} value={state.realityFallbackDownloadBytesPerSec} onChange={(e) => updateStructured({ realityFallbackDownloadBytesPerSec: e.target.value })} placeholder="disabled" />
                  </Field>
                  <Field label="Fallback download burst" hint="Additional response bytes allowed immediately before sustained pacing.">
                    <Input type="number" min={0} value={state.realityFallbackDownloadBurstBytesPerSec} onChange={(e) => updateStructured({ realityFallbackDownloadBurstBytesPerSec: e.target.value })} placeholder="0" />
                  </Field>
                </div>
              </>
            ) : null}

            {state.security === "shadowtls" ? <div className="configurator-grid"><Field label="Password"><Input type="password" value={state.shadowTlsPassword} onChange={(e) => updateStructured({ shadowTlsPassword: e.target.value })} /></Field><Field label="TLS camouflage destination"><Input value={state.shadowTlsDest} onChange={(e) => updateStructured({ shadowTlsDest: e.target.value })} placeholder="www.apple.com:443" /></Field><Field label="Version"><Input type="number" min={3} max={3} value={state.shadowTlsVersion} onChange={(e) => updateStructured({ shadowTlsVersion: e.target.value })} /></Field></div> : null}

            {state.security === "none" ? <p className="field-hint">Use only on trusted links. TLS or REALITY is usually the better default for public-facing listeners.</p> : null}
          </section>
        ) : null}

        {activeTab === "sniffing" ? (
          <section className="drawer-card configurator-section">
            <Switch checked={state.sniffingEnabled} onChange={(sniffingEnabled) => updateStructured({ sniffingEnabled })} label="Sniffing enabled" />
            <div className="field">
              <span className="field-label">Destination override</span>
              <div className="toggle-grid">
                {sniffingOptions.map((item) => {
                  const active = state.sniffingDestOverride.includes(item);
                  return (
                    <button
                      key={item}
                      type="button"
                      className={`toggle-chip ${active ? "toggle-chip-active" : ""}`}
                      onClick={() =>
                        updateStructured({
                          sniffingDestOverride: active
                            ? state.sniffingDestOverride.filter((value) => value !== item)
                            : [...state.sniffingDestOverride, item]
                        })
                      }
                    >
                      {item}
                    </button>
                  );
                })}
              </div>
            </div>
            <div className="configurator-grid">
              <Switch checked={state.sniffingMetadataOnly} onChange={(sniffingMetadataOnly) => updateStructured({ sniffingMetadataOnly })} label="Metadata only" />
              <Switch checked={state.sniffingRouteOnly} onChange={(sniffingRouteOnly) => updateStructured({ sniffingRouteOnly })} label="Route only" />
            </div>
            <div className="configurator-grid">
              <Field label="Max connections">
                <Input value={state.maxConnections} onChange={(e) => updateStructured({ maxConnections: e.target.value })} placeholder="10000" />
              </Field>
              <Field label="Max handshake seconds">
                <Input value={state.maxHandshakeSeconds} onChange={(e) => updateStructured({ maxHandshakeSeconds: e.target.value })} placeholder="10" />
              </Field>
            </div>
          </section>
        ) : null}

        {jsonErrors.length > 0 ? (
          <div className="error-line inline-error">
            <AlertCircle size={15} />
            <span>The stored typed configuration is invalid and cannot be saved safely.</span>
          </div>
        ) : null}
        {validationIssues.length > 0 ? (
          <div className="error-line inline-error">
            <AlertCircle size={15} />
            <span>{validationIssues[0].message}</span>
          </div>
        ) : null}
        {compatibilityNotice ? (
          <CompatibilityNotice tone={compatibilityNotice.tone} message={compatibilityNotice.message} />
        ) : null}

        {editing && inboundsCount <= 1 ? <p className="field-hint">Create another inbound before deleting this one.</p> : null}
      </div>
      <div className="drawer-foot">
        {editing ? (
          <Button
            variant="danger"
            icon={<Trash2 size={16} />}
            onClick={() => onDelete(editing.id)}
            loading={busy}
            disabled={!canDelete}
            title={canDelete ? "Delete inbound" : "Create another inbound before deleting this one"}
          >
            Delete
          </Button>
        ) : (
          <Button variant="ghost" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
        )}
        <Button variant="primary" icon={<Save size={16} />} onClick={submit} loading={busy} disabled={saveDisabled}>
          Save Inbound
        </Button>
      </div>
      {tlsSelfSignedOpen ? (
        <div className="dialog-backdrop" role="presentation">
          <section className="dialog-panel" role="dialog" aria-modal="true" aria-labelledby="tls-self-signed-title">
            <div className="dialog-head">
              <div>
                <h3 id="tls-self-signed-title">Generate Self-Signed TLS Certificate</h3>
                <p>Black UI writes the certificate and key on the server, then fills the TLS paths for this inbound.</p>
              </div>
              <IconButton label="Close self-signed TLS helper" onClick={() => setTlsSelfSignedOpen(false)}>
                <X size={18} />
              </IconButton>
            </div>
            <div className="dialog-body">
              <div className="compatibility-notice compatibility-notice-warning">
                <AlertCircle size={15} />
                <span>Self-signed TLS needs client trust setup or insecure/skip-cert-verify mode. For a real domain, a public CA certificate is the normal path.</span>
              </div>
              <div className="configurator-grid">
                <Field label="Server name or IP" hint="Use the exact value clients will connect to.">
                  <Input value={tlsSelfSigned.serverName} onChange={(e) => updateTlsSelfSigned({ serverName: e.target.value })} placeholder="example.com or 203.0.113.10" />
                </Field>
                <Field label="Valid days" hint="Clamped to 3650 days by the backend.">
                  <Input type="number" min={1} max={3650} value={tlsSelfSigned.days} onChange={(e) => updateTlsSelfSigned({ days: e.target.value })} />
                </Field>
                <Field label="Default certificate file" hint="The backend response is applied after generation.">
                  <Input value={tlsSelfSignedPreview.certificateFile} readOnly />
                </Field>
                <Field label="Default key file" hint="The backend response is applied after generation.">
                  <Input value={tlsSelfSignedPreview.keyFile} readOnly />
                </Field>
              </div>
              {tlsSelfSignedMessage ? <p className="copy-feedback">{tlsSelfSignedMessage}</p> : null}
            </div>
            <div className="dialog-actions">
              <Button type="button" variant="ghost" onClick={() => setTlsSelfSignedOpen(false)} disabled={tlsSelfSignedBusy}>
                Cancel
              </Button>
              <Button type="button" variant="primary" icon={<Save size={16} />} onClick={generateTlsSelfSignedCertificate} loading={tlsSelfSignedBusy} disabled={busy || tlsSelfSignedBusy}>
                Generate on Server
              </Button>
            </div>
          </section>
        </div>
      ) : null}
    </aside>
  );
}

function capabilityLabel(item: { label: string; status: string }) {
  return item.status === "supported" ? item.label : `${item.label} (${item.status})`;
}

function CompatibilityNotice({ tone, message }: { tone: "info" | "warning"; message: string }) {
  return (
    <div className={`compatibility-notice compatibility-notice-${tone}`}>
      <AlertCircle size={15} />
      <span>{message}</span>
    </div>
  );
}
