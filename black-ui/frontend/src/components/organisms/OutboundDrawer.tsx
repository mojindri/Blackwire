import { AlertCircle, Save, Trash2, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import type { CapabilityMap, Outbound, OutboundInput } from "../../lib/types";
import {
  buildOutboundInput,
  createOutboundEditorState,
  outboundSummary,
  replaceOutboundSlice,
  syncOutboundAfterStructuredChange,
  validateOutboundState,
  type OutboundEditorState,
  type OutboundSliceKey
} from "../../lib/outboundConfigurator";
import { Button } from "../atoms/Button";
import { IconButton } from "../atoms/IconButton";
import { Input, Select, Textarea } from "../atoms/Input";
import { Switch } from "../atoms/Switch";
import { Field } from "../molecules/Field";

type TabKey = "basic" | "protocol" | "transport" | "security" | "advanced";

const tabOrder: Array<{ key: TabKey; label: string }> = [
  { key: "basic", label: "Basic" },
  { key: "protocol", label: "Protocol" },
  { key: "transport", label: "Transport" },
  { key: "security", label: "Security" },
  { key: "advanced", label: "Advanced" }
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

  useEffect(() => {
    setState(createOutboundEditorState(editing));
    setActiveTab("basic");
  }, [editing]);

  const protocolOptions = useMemo(
    () =>
      capabilities?.protocols.filter((item) =>
        ["freedom", "vless", "vmess", "trojan", "shadowsocks", "hysteria2"].includes(item.key)
      ) ?? [
        { key: "freedom", label: "Freedom", status: "supported", notes: "" },
        { key: "vless", label: "VLESS", status: "supported", notes: "" },
        { key: "vmess", label: "VMess", status: "supported", notes: "" },
        { key: "trojan", label: "Trojan", status: "supported", notes: "" },
        { key: "shadowsocks", label: "Shadowsocks", status: "supported", notes: "" },
        { key: "hysteria2", label: "Hysteria2", status: "supported", notes: "" }
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

  const currentSummary = editing ? outboundSummary(editing) : { network: state.network, security: state.security, detail: "" };
  const jsonErrors = [state.settings, state.streamSettings].filter((slice) => slice.error);
  const validationIssues = validateOutboundState(state);
  const saveDisabled = busy || jsonErrors.length > 0 || validationIssues.length > 0;

  const updateStructured = (patch: Partial<OutboundEditorState>) => {
    setState((current) => syncOutboundAfterStructuredChange({ ...current, ...patch }));
  };

  const updateSlice = (key: OutboundSliceKey, text: string) => {
    setState((current) => replaceOutboundSlice(current, key, text));
  };

  const submit = () => {
    const input = buildOutboundInput(state);
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
              ? "Structured outbound configuration with protocol-aware tabs and advanced JSON fallback."
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
                    <option key={item.key} value={item.key}>
                      {item.label}
                    </option>
                  ))}
                </Select>
              </Field>
            </div>
          </section>
        ) : null}

        {activeTab === "protocol" ? (
          <section className="drawer-card configurator-section">
            {state.protocol === "freedom" ? <p className="field-hint">Freedom is the direct outbound. It usually needs no protocol-level settings.</p> : null}

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

            {state.protocol === "hysteria2" ? (
              <Field label="Server" hint="Client-side Hysteria2 target, for example 127.0.0.1:443 or [::1]:443">
                <Input value={state.server} onChange={(e) => updateStructured({ server: e.target.value })} placeholder="127.0.0.1:443" />
              </Field>
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
              <Field label="Path">
                <Input value={state.splitHttpPath} onChange={(e) => updateStructured({ splitHttpPath: e.target.value })} placeholder="/packet" />
              </Field>
            ) : null}

            {state.network === "kcp" || state.network === "quic" ? (
              <p className="field-hint">KCP and legacy QUIC often need extra tuning. Use Advanced for the rest of the transport-specific keys.</p>
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

            {state.security === "none" ? <p className="field-hint">No extra security wrapper. TLS or REALITY is usually the better fit for remote proxy outbounds.</p> : null}
          </section>
        ) : null}

        {activeTab === "advanced" ? (
          <section className="drawer-card configurator-section">
            <AdvancedSlice
              label="Settings JSON"
              hint="Protocol-specific outbound settings. Structured controls own the common fields and preserve the rest."
              value={state.settings.text}
              error={state.settings.error}
              placeholder='{"address":"1.2.3.4","port":443}'
              onChange={(text) => updateSlice("settings", text)}
            />
            <AdvancedSlice
              label="Stream settings JSON"
              hint="Transport and security JSON. Use this for custom keys not surfaced by the structured tabs."
              value={state.streamSettings.text}
              error={state.streamSettings.error}
              placeholder='{"network":"tcp","security":"tls"}'
              onChange={(text) => updateSlice("streamSettings", text)}
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
