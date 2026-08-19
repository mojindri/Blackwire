import { AlertCircle, Save, Trash2, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { defaultHysteria2CongestionTuning, defaultHysteria2TransportTuning, hasCustomHysteria2Congestion, HYSTERIA2_SIMPLE_CONGESTION_MODES } from "../../lib/hysteria2Tuning";
import type { CapabilityMap, Outbound, OutboundInput } from "../../lib/types";
import {
  buildOutboundInput,
  createOutboundEditorState,
  outboundSummary,
  syncOutboundAfterStructuredChange,
  validateOutboundState,
  type OutboundEditorState
} from "../../lib/outboundConfigurator";
import { Button } from "../atoms/Button";
import { IconButton } from "../atoms/IconButton";
import { Input, Select } from "../atoms/Input";
import { Switch } from "../atoms/Switch";
import { Field } from "../molecules/Field";
import { SplitHttpFields } from "./SplitHttpFields";
import { Hysteria2TuningFields } from "./Hysteria2TuningFields";

type TabKey = "basic" | "protocol" | "transport" | "security";

const tabOrder: Array<{ key: TabKey; label: string }> = [
  { key: "basic", label: "Basic" },
  { key: "protocol", label: "Protocol" },
  { key: "transport", label: "Transport" },
  { key: "security", label: "Security" }
];

export function OutboundDrawer({
  editing,
  capabilities,
  busy,
  onClose,
  onCreate,
  onUpdate,
  onDelete
}: {
  editing: Outbound | null;
  capabilities: CapabilityMap | null;
  busy: boolean;
  onClose: () => void;
  onCreate: (input: OutboundInput) => void;
  onUpdate: (id: number, input: OutboundInput) => void;
  onDelete: (id: number) => void;
}) {
  const [activeTab, setActiveTab] = useState<TabKey>("basic");
  const [state, setState] = useState<OutboundEditorState>(() => createOutboundEditorState(editing));
  const stateRef = useRef(state);

  useEffect(() => {
    const next = createOutboundEditorState(editing);
    stateRef.current = next;
    setState(next);
    setActiveTab("basic");
  }, [editing]);

  const protocolOptions = useMemo(
    () =>
      capabilities?.protocols.filter((item) =>
        ["freedom", "vless", "vmess", "trojan", "shadowsocks", "hysteria2", "tuic"].includes(item.key)
      ) ?? [
        { key: "freedom", label: "Freedom", status: "supported", notes: "" },
        { key: "vless", label: "VLESS", status: "supported", notes: "" },
        { key: "vmess", label: "VMess", status: "supported", notes: "" },
        { key: "trojan", label: "Trojan", status: "supported", notes: "" },
        { key: "shadowsocks", label: "Shadowsocks", status: "supported", notes: "" },
        { key: "hysteria2", label: "Hysteria2", status: "supported", notes: "" },
        { key: "tuic", label: "TUIC v5", status: "supported", notes: "QUIC v5 TCP and UDP" }
      ],
    [capabilities]
  );
  const transportOptions = useMemo(
    () => {
      const visible =
        capabilities?.transports.filter((item) =>
          ["tcp", "ws", "grpc", "httpupgrade", "splithttp"].includes(item.key)
        ) ?? [
          { key: "tcp", label: "TCP", status: "supported", notes: "" },
          { key: "ws", label: "WebSocket", status: "supported", notes: "" },
          { key: "grpc", label: "gRPC", status: "supported", notes: "" },
          { key: "httpupgrade", label: "HTTPUpgrade", status: "supported", notes: "" },
          { key: "splithttp", label: "SplitHTTP", status: "supported", notes: "" }
        ];
      if (
        (state.protocol === "hysteria2" || state.protocol === "tuic") &&
        !visible.some((item) => item.key === "quic")
      ) {
        return [
          ...visible,
          { key: "quic", label: "Native QUIC", status: "supported", notes: "Built into this protocol" }
        ];
      }
      const current = capabilities?.transports.find((item) => item.key === state.network);
      if (current && !visible.some((item) => item.key === current.key)) {
        return [...visible, current];
      }
      if (!current && state.network !== "quic" && state.network && !visible.some((item) => item.key === state.network)) {
        return [...visible, { key: state.network, label: state.network, status: "deprecated", notes: "Legacy transport retained for editing existing configs" }];
      }
      return visible;
    },
    [capabilities, state.network, state.protocol]
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

  const currentSummary = editing ? outboundSummary(editing) : { network: state.network, security: state.security, detail: "" };
  const jsonErrors = [state.settings, state.streamSettings].filter((slice) => slice.error);
  const validationIssues = validateOutboundState(state);
  const saveDisabled = busy || jsonErrors.length > 0 || validationIssues.length > 0;
  const hysteria2CustomCongestion = state.protocol === "hysteria2" && hasCustomHysteria2Congestion(state);
  const hysteria2TransportOverrides = state.protocol === "hysteria2" && state.hysteria2TransportOverrides;

  const updateStructured = (patch: Partial<OutboundEditorState>) => {
    const next = syncOutboundAfterStructuredChange({ ...stateRef.current, ...patch });
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
    const latestJsonErrors = [latest.settings, latest.streamSettings].filter((slice) => slice.error);
    if (busy || latestJsonErrors.length > 0 || validateOutboundState(latest).length > 0) return;
    const input = buildOutboundInput(latest);
    if (editing) onUpdate(editing.id, input);
    else onCreate(input);
    onClose();
  };

  return (
    <aside className="drawer drawer-wide">
      <div className="drawer-head">
        <div>
          <h2>{editing ? editing.tag : "New outbound"}</h2>
          <p>
            {editing
              ? "Structured outbound configuration with protocol-aware typed fields."
              : "Create a new outbound with guided protocol, transport, and security settings."}
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
              <strong>{state.tag || "Untitled outbound"}</strong>
              <span>{currentSummary.detail || "No destination configured yet"}</span>
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

        <div className="configurator-tabs" role="tablist" aria-label="Outbound editor sections">
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
                      {item.status === "supported" ? item.label : `${item.label} (${item.status})`}
                    </option>
                  ))}
                </Select>
              </Field>
            </div>
          </section>
        ) : null}

        {activeTab === "protocol" ? (
          <section className="drawer-card configurator-section">
            {state.protocol === "freedom" ? (
              <>
                <div className="configurator-grid">
                  <Field label="IP strategy" hint="Prefer IPv4 keeps IPv6 fallback. IPv4 only blocks IPv6 domain results for VPSes with broken IPv6 egress.">
                    <Select value={state.freedomIpStrategy} onChange={(e) => updateStructured({ freedomIpStrategy: e.target.value })}>
                      <option value="auto">Auto</option>
                      <option value="UseIP">Use IP</option>
                      <option value="PreferIPv4">Prefer IPv4</option>
                      <option value="PreferIPv6">Prefer IPv6</option>
                      <option value="UseIPv4">IPv4 only</option>
                      <option value="UseIPv6">IPv6 only</option>
                    </Select>
                  </Field>
                  <Switch
                    checked={state.freedomRejectIpv6Literal}
                    onChange={(freedomRejectIpv6Literal) => updateStructured({ freedomRejectIpv6Literal })}
                    label="Reject IPv6 literal destinations"
                  />
                </div>
                <p className="field-hint">Use Prefer IPv4 first. If this VPS still times out on literal IPv6 destinations, enable the IPv6 literal guard.</p>
              </>
            ) : null}

            {["vless", "vmess", "trojan", "shadowsocks"].includes(state.protocol) ? (
              <div className="configurator-grid">
                <Field label="Address">
                <Input value={state.address} onChange={(e) => updateStructured({ address: e.target.value })} placeholder="1.2.3.4" />
              </Field>
              <Field label="Port">
                <Input value={state.port} onChange={(e) => updateStructured({ port: e.target.value })} placeholder="443" />
                </Field>
              </div>
            ) : null}

            {state.protocol === "vless" || state.protocol === "vmess" ? (
              <Field label="User ID" hint="Maps to settings.users[0].id. Any extra user keys in JSON are preserved.">
                <Input value={state.userId} onChange={(e) => updateStructured({ userId: e.target.value })} placeholder="550e8400-e29b-41d4-a716-446655440000" />
              </Field>
            ) : null}

            {state.protocol === "trojan" || state.protocol === "shadowsocks" ? (
              <Field label="Password">
                <Input value={state.password} onChange={(e) => updateStructured({ password: e.target.value })} placeholder="secret-password" />
              </Field>
            ) : null}

            {state.protocol === "shadowsocks" ? (
              <Field label="Method">
                <Input value={state.method} onChange={(e) => updateStructured({ method: e.target.value })} placeholder="2022-blake3-aes-128-gcm" />
              </Field>
            ) : null}

            {state.protocol === "hysteria2" || state.protocol === "tuic" ? (
              <Field label="Server" hint={`Client-side ${state.protocol === "tuic" ? "TUIC v5" : "Hysteria2"} target, for example 127.0.0.1:443 or [::1]:443`}>
                <Input value={state.server} onChange={(e) => updateStructured({ server: e.target.value })} placeholder="127.0.0.1:443" />
              </Field>
            ) : null}

            {state.protocol === "hysteria2" ? (
              <>
                <div className="configurator-grid">
                  <Field label="Auth">
                    <Input value={state.hysteria2Auth} onChange={(e) => updateStructured({ hysteria2Auth: e.target.value })} placeholder="shared-secret" />
                  </Field>
                  <Field label="Server name" hint="SNI for the Hysteria2 TLS/QUIC connection.">
                    <Input value={state.hysteria2ServerName} onChange={(e) => updateStructured({ hysteria2ServerName: e.target.value })} placeholder="example.com" />
                  </Field>
                  <Switch checked={state.hysteria2SkipCertVerify} onChange={(hysteria2SkipCertVerify) => updateStructured({ hysteria2SkipCertVerify })} label="Skip certificate verification" />
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
                  <Field label="Min ACK rate">
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

            {state.protocol === "tuic" ? (
              <div className="configurator-grid">
                <Field label="UUID" hint="Maps to settings.uuid for TUIC v5 authentication.">
                  <Input value={state.userId} onChange={(e) => updateStructured({ userId: e.target.value })} placeholder="550e8400-e29b-41d4-a716-446655440000" />
                </Field>
                <Field label="Password">
                  <Input value={state.password} onChange={(e) => updateStructured({ password: e.target.value })} placeholder="secret-password" />
                </Field>
              </div>
            ) : null}
          </section>
        ) : null}

        {activeTab === "transport" ? (
          <section className="drawer-card configurator-section">
            <div className="configurator-grid">
              <Field label="Network">
                <Select value={state.network} onChange={(e) => updateStructured({ network: e.target.value })}>
                  {transportOptions.map((item) => (
                    <option key={item.key} value={item.key}>
                      {item.label}
                    </option>
                  ))}
                </Select>
              </Field>
            </div>

            {state.network === "ws" ? (
              <div className="configurator-grid">
                <Field label="Path">
                  <Input value={state.wsPath} onChange={(e) => updateStructured({ wsPath: e.target.value })} placeholder="/proxy" />
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

            {state.protocol === "hysteria2" ? <>
              <Switch checked={hysteria2TransportOverrides} onChange={(enabled) => {
                updateStructured({
                  hysteria2TransportOverrides: enabled,
                  ...(!enabled ? defaultHysteria2TransportTuning() : {})
                });
              }} label="Override Hysteria2 transport defaults" />
              <p className="field-hint">Off inherits Blackwire's global QUIC, datagram, and FEC behavior.</p>
            </> : null}
            {hysteria2TransportOverrides ? <Hysteria2TuningFields direction="outbound" value={state} onChange={updateStructured} /> : null}
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
              <div className="configurator-grid">
                <Field label="Server name">
                  <Input value={state.tlsServerName} onChange={(e) => updateStructured({ tlsServerName: e.target.value })} placeholder="example.com" />
                </Field>
                <Field label="ALPN">
                  <Input value={state.tlsAlpn} onChange={(e) => updateStructured({ tlsAlpn: e.target.value })} placeholder="h2, http/1.1" />
                </Field>
                <Switch checked={state.tlsAllowInsecure} onChange={(tlsAllowInsecure) => updateStructured({ tlsAllowInsecure })} label="Allow insecure certificates" />
                <Field label="Certificate file">
                  <Input value={state.tlsCertificateFile} onChange={(e) => updateStructured({ tlsCertificateFile: e.target.value })} placeholder="/etc/blackwire/fullchain.pem" />
                </Field>
                <Field label="Key file">
                  <Input value={state.tlsKeyFile} onChange={(e) => updateStructured({ tlsKeyFile: e.target.value })} placeholder="/etc/blackwire/privkey.pem" />
                </Field>
              </div>
            ) : null}

            {state.security === "reality" ? (
              <div className="configurator-grid">
                <Field label="Server name">
                  <Input value={state.realityServerName} onChange={(e) => updateStructured({ realityServerName: e.target.value })} placeholder="www.cloudflare.com" />
                </Field>
                <Field label="Public key">
                  <Input value={state.realityPublicKey} onChange={(e) => updateStructured({ realityPublicKey: e.target.value })} placeholder="base64-x25519-public-key" />
                </Field>
                <Field label="Short ID">
                  <Input value={state.realityShortId} onChange={(e) => updateStructured({ realityShortId: e.target.value })} placeholder="6ba85179e30d4fc2" />
                </Field>
                <Field label="Fingerprint">
                  <Input value={state.realityFingerprint} onChange={(e) => updateStructured({ realityFingerprint: e.target.value })} placeholder="chrome" />
                </Field>
                <Field label="Spider X">
                  <Input value={state.realitySpiderX} onChange={(e) => updateStructured({ realitySpiderX: e.target.value })} placeholder="/" />
                </Field>
              </div>
            ) : null}

            {state.security === "shadowtls" ? <div className="configurator-grid"><Field label="Password"><Input type="password" value={state.shadowTlsPassword} onChange={(e) => updateStructured({ shadowTlsPassword: e.target.value })} /></Field><Field label="TLS camouflage destination"><Input value={state.shadowTlsDest} onChange={(e) => updateStructured({ shadowTlsDest: e.target.value })} placeholder="www.apple.com:443" /></Field><Field label="Version"><Input type="number" min={3} max={3} value={state.shadowTlsVersion} onChange={(e) => updateStructured({ shadowTlsVersion: e.target.value })} /></Field></div> : null}

            {state.security === "none" ? <p className="field-hint">No extra security wrapper. TLS or REALITY is usually the better fit for remote proxy outbounds.</p> : null}
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
      </div>
      <div className="drawer-foot">
        {editing ? (
          <Button variant="danger" icon={<Trash2 size={16} />} onClick={() => onDelete(editing.id)} loading={busy}>
            Delete
          </Button>
        ) : (
          <Button variant="ghost" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
        )}
        <Button variant="primary" icon={<Save size={16} />} onClick={submit} loading={busy} disabled={saveDisabled}>
          Save Outbound
        </Button>
      </div>
    </aside>
  );
}
