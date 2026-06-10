import { AlertCircle, Save, Trash2, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import type { CapabilityMap, Inbound, InboundInput } from "../../lib/types";
import {
  buildInboundInput,
  createInboundEditorState,
  inboundSummary,
  replaceSlice,
  syncAfterStructuredChange,
  validateInboundState,
  type InboundEditorState,
  type SliceKey
} from "../../lib/inboundConfigurator";
import { Button } from "../atoms/Button";
import { IconButton } from "../atoms/IconButton";
import { Input, Select, Textarea } from "../atoms/Input";
import { Switch } from "../atoms/Switch";
import { Field } from "../molecules/Field";

type TabKey = "basic" | "protocol" | "transport" | "security" | "sniffing" | "advanced";

const sniffingOptions = ["http", "tls", "quic", "fakedns"];
const tabOrder: Array<{ key: TabKey; label: string }> = [
  { key: "basic", label: "Basic" },
  { key: "protocol", label: "Protocol" },
  { key: "transport", label: "Transport" },
  { key: "security", label: "Security" },
  { key: "sniffing", label: "Sniffing" },
  { key: "advanced", label: "Advanced" }
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

  useEffect(() => {
    setState(createInboundEditorState(editing));
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
    () =>
      capabilities?.transports.filter((item) =>
        ["tcp", "ws", "grpc", "httpupgrade", "splithttp", "kcp", "quic"].includes(item.key)
      ) ?? [
        { key: "tcp", label: "TCP", status: "supported", notes: "" },
        { key: "ws", label: "WebSocket", status: "supported", notes: "" },
        { key: "grpc", label: "gRPC", status: "supported", notes: "" },
        { key: "httpupgrade", label: "HTTPUpgrade", status: "supported", notes: "" },
        { key: "splithttp", label: "SplitHTTP", status: "supported", notes: "" },
        { key: "kcp", label: "mKCP", status: "supported", notes: "" },
        { key: "quic", label: "QUIC", status: "supported", notes: "" }
      ],
    [capabilities]
  );
  const securityOptions = useMemo(
    () =>
      capabilities?.security.filter((item) => ["none", "tls", "reality"].includes(item.key)) ?? [
        { key: "none", label: "No security", status: "supported", notes: "" },
        { key: "tls", label: "TLS", status: "supported", notes: "" },
        { key: "reality", label: "REALITY", status: "supported", notes: "" }
      ],
    [capabilities]
  );

  const currentSummary = editing ? inboundSummary(editing) : { network: state.network, security: state.security, detail: "" };
  const jsonErrors = [state.settings, state.streamSettings, state.sniffing, state.limits].filter((slice) => slice.error);
  const validationIssues = validateInboundState(state);
  const canDelete = !busy && inboundsCount > 1;
  const saveDisabled = busy || jsonErrors.length > 0 || validationIssues.length > 0;

  const updateStructured = (patch: Partial<InboundEditorState>) => {
    setState((current) => syncAfterStructuredChange({ ...current, ...patch }));
  };

  const updateSlice = (key: SliceKey, text: string) => {
    setState((current) => replaceSlice(current, key, text));
  };

  const submit = () => {
    const input = buildInboundInput(state);
    if (editing) {
      onUpdate(editing.id, input);
    } else {
      onCreate(input);
    }
    onClose();
  };

  return (
    <aside className="drawer drawer-wide">
      <div className="drawer-head">
        <div>
          <h2>{editing ? editing.tag : "New inbound"}</h2>
          <p>
            {editing
              ? "Structured inbound configuration with protocol-aware tabs and advanced JSON fallback."
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
                      {item.status === "supported" ? item.label : `${item.label} (${item.status})`}
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
            {state.protocol === "vless" || state.protocol === "vmess" ? (
              <Field label={state.protocol === "vless" ? "Decryption" : "Encryption"}>
                <Input
                  value={state.decryption}
                  onChange={(e) => updateStructured({ decryption: e.target.value })}
                  placeholder={state.protocol === "vless" ? "none" : "auto"}
                />
              </Field>
            ) : null}
            {state.protocol === "shadowsocks" ? (
              <Field label="Method" hint="Inbound-level method only. Managed users still live in Users.">
                <Input value={state.shadowsocksMethod} onChange={(e) => updateStructured({ shadowsocksMethod: e.target.value })} placeholder="2022-blake3-aes-128-gcm" />
              </Field>
            ) : null}
            {state.protocol === "trojan" ? (
              <p className="field-hint">Trojan client secrets continue to be managed through Users. Use Advanced only for extra inbound-level keys.</p>
            ) : null}
            {state.protocol === "hysteria2" ? (
              <p className="field-hint">Hysteria2 often needs extra tuning. Start with Transport and Security, then use Advanced for anything custom.</p>
            ) : null}
            {state.protocol === "socks" || state.protocol === "http" ? (
              <p className="field-hint">Listener basics are handled here. Auth and less-common protocol knobs stay available under Advanced.</p>
            ) : null}
            {!["vless", "vmess", "shadowsocks", "trojan", "hysteria2", "socks", "http"].includes(state.protocol) ? (
              <p className="field-hint">This protocol is still editable through Advanced without losing custom keys.</p>
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
              <Field label="Path">
                <Input value={state.splitHttpPath} onChange={(e) => updateStructured({ splitHttpPath: e.target.value })} placeholder="/packet" />
              </Field>
            ) : null}

            {state.network === "kcp" ? (
              <div className="configurator-grid">
                <Field label="Header">
                  <Input value={state.kcpHeader} onChange={(e) => updateStructured({ kcpHeader: e.target.value })} placeholder="srtp" />
                </Field>
                <Field label="MTU">
                  <Input value={state.kcpMtu} onChange={(e) => updateStructured({ kcpMtu: e.target.value })} placeholder="1350" />
                </Field>
                <Field label="TTI">
                  <Input value={state.kcpTti} onChange={(e) => updateStructured({ kcpTti: e.target.value })} placeholder="20" />
                </Field>
                <Field label="Uplink capacity">
                  <Input value={state.kcpUplinkCapacity} onChange={(e) => updateStructured({ kcpUplinkCapacity: e.target.value })} placeholder="5" />
                </Field>
                <Field label="Downlink capacity">
                  <Input value={state.kcpDownlinkCapacity} onChange={(e) => updateStructured({ kcpDownlinkCapacity: e.target.value })} placeholder="20" />
                </Field>
                <Field label="Read buffer size">
                  <Input value={state.kcpReadBufferSize} onChange={(e) => updateStructured({ kcpReadBufferSize: e.target.value })} placeholder="2" />
                </Field>
                <Field label="Write buffer size">
                  <Input value={state.kcpWriteBufferSize} onChange={(e) => updateStructured({ kcpWriteBufferSize: e.target.value })} placeholder="2" />
                </Field>
                <Switch checked={state.kcpCongestion} onChange={(kcpCongestion) => updateStructured({ kcpCongestion })} label="Enable congestion control" />
              </div>
            ) : null}

            {state.network === "quic" ? (
              <p className="field-hint">QUIC transport is available here as a network choice. Endpoint-level transport stays structured; top-level QUIC socket tuning still belongs in Advanced Config.</p>
            ) : null}
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

        {activeTab === "advanced" ? (
          <section className="drawer-card configurator-section">
            <AdvancedSlice
              label="Settings JSON"
              hint="Protocol-specific inbound settings. Managed users are merged separately, so clients stay out of this editor."
              value={state.settings.text}
              error={state.settings.error}
              placeholder='{"decryption":"none"}'
              onChange={(text) => updateSlice("settings", text)}
            />
            <AdvancedSlice
              label="Stream settings JSON"
              hint="Transport and security JSON. Structured tabs own common keys and preserve the rest."
              value={state.streamSettings.text}
              error={state.streamSettings.error}
              placeholder='{"network":"ws","security":"tls"}'
              onChange={(text) => updateSlice("streamSettings", text)}
            />
            <AdvancedSlice
              label="Sniffing JSON"
              value={state.sniffing.text}
              error={state.sniffing.error}
              placeholder='{"enabled":true,"destOverride":["http","tls"]}'
              onChange={(text) => updateSlice("sniffing", text)}
            />
            <AdvancedSlice
              label="Limits JSON"
              value={state.limits.text}
              error={state.limits.error}
              placeholder='{"maxConnections":10000,"maxHandshakeSeconds":10}'
              onChange={(text) => updateSlice("limits", text)}
            />
          </section>
        ) : null}

        {jsonErrors.length > 0 ? (
          <div className="error-line inline-error">
            <AlertCircle size={15} />
            <span>Fix invalid JSON in Advanced before saving.</span>
          </div>
        ) : null}
        {validationIssues.length > 0 ? (
          <div className="error-line inline-error">
            <AlertCircle size={15} />
            <span>{validationIssues[0].message}</span>
          </div>
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
    </aside>
  );
}

function AdvancedSlice({
  label,
  hint,
  value,
  error,
  placeholder,
  onChange
}: {
  label: string;
  hint?: string;
  value: string;
  error: string;
  placeholder: string;
  onChange: (text: string) => void;
}) {
  return (
    <Field label={label} hint={hint}>
      <div className="advanced-slice">
        <Textarea rows={7} value={value} onChange={(e) => onChange(e.target.value)} placeholder={placeholder} />
        {error ? <div className="field-error">{error}</div> : null}
      </div>
    </Field>
  );
}
