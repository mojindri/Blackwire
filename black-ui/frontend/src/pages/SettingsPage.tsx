import { useEffect, useState } from "react";
import { ChevronDown, Gauge, Save, Settings2, Shield, Terminal } from "lucide-react";
import type { CoreSettings, Settings } from "../lib/types";
import { Button } from "../components/atoms/Button";
import { Input, Select, Textarea } from "../components/atoms/Input";
import { Switch as BaseSwitch } from "../components/atoms/Switch";
import { Field as BaseField } from "../components/molecules/Field";
import { getSettingsHelp } from "../lib/settingsHelp";
import { applyOptimizationMode, defaultFastSettings, optimizationModeFromSettings, optimizationStatusFromSettings, type OptimizationMode } from "../lib/optimizationMode";

const optionalNumber = (value: string) => value === "" ? null : Number(value);
const lines = (value: string) => value.split("\n").map((item) => item.trim()).filter(Boolean);
const defaultQuic = { reusePort: false, endpoints: 1, recvBufferBytes: 8388608, sendBufferBytes: 8388608, maxDatagramSize: "auto" as number | string };
const defaultDatagram = { enabled: true, udpOverDatagram: true, tunPacketsOverDatagram: false, policy: "standard" as const, maxQueueDelayMs: 25, fastDnsRetry: false, fastDnsRetryDelayMs: 20 };
const defaultFec = { mode: "auto" as const, maxOverheadPercent: 20, protectClasses: ["dns", "interactive", "control"], avoidBulkTcp: true, disableForSequentialDns: true, minConcurrencyForBlockFec: 4, maxGenerationPackets: 4, maxGenerationDelayMs: 20, recoveryDeadlineMs: 100, dedupWindowPackets: 1024 };
const defaultBudget: NonNullable<CoreSettings["budget"]> = { maxProtocolLayers: 3, allowSniffing: false, allowFakeIp: false, maxRouteRules: 50, preferDirectCopy: true };
const defaultVision: NonNullable<CoreSettings["vision"]> = { directCopy: "auto", maxPacketsToFilter: 8, allowSpliceAfterDirect: true };
const defaultBoost: NonNullable<CoreSettings["firstPacketBoost"]> = { enabled: false, dns: true, sendEarlyPayload: true };

function Field(props: React.ComponentProps<typeof BaseField>) { return <BaseField {...props} help={props.help ?? getSettingsHelp(props.label)} />; }
function Switch(props: React.ComponentProps<typeof BaseSwitch>) { return <BaseSwitch {...props} help={props.help ?? getSettingsHelp(props.label)} />; }

export function SettingsPage({ settings, coreSettings, busy, onSave, onSaveCore }: { settings: Settings | null; coreSettings: CoreSettings | null; busy: boolean; onSave: (settings: Settings) => void; onSaveCore: (settings: CoreSettings) => void }) {
  const [panel, setPanel] = useState(settings);
  const [core, setCore] = useState(coreSettings);
  useEffect(() => setPanel(settings), [settings]);
  useEffect(() => setCore(coreSettings), [coreSettings]);
  if (!panel || !core) return <div className="page">Loading settings...</div>;

  return <div className="page settings-page">
    <div className="page-title"><h1>Settings</h1><p>Control Black UI automation and the Blackwire runtime from one place.</p></div>

    <SettingsSection icon={<Settings2 size={19} />} title="Control panel" copy="Panel access, subscriptions, and adaptive automation." recommendation="Keep automatic firewall changes off unless this host is dedicated to Blackwire. Use recommend mode before enabling automatic tuning." impact="Applies immediately" details="Public and subscription addresses determine generated links. Enforcement periodically reconciles panel state. Adaptive tuning can recommend or automatically apply measured routing changes." defaultOpen>
      <div className="settings-grid">
        <Switch checked={panel.firewallAutoOpen} onChange={(firewallAutoOpen) => setPanel({ ...panel, firewallAutoOpen })} label="Auto-open UFW ports" />
        <Switch checked={panel.adaptiveRoutingEnabled} onChange={(adaptiveRoutingEnabled) => setPanel({ ...panel, adaptiveRoutingEnabled })} label="Auto adaptive routing" />
        <Field label="Public base URL"><Input value={panel.publicBaseUrl} onChange={(e) => setPanel({ ...panel, publicBaseUrl: e.target.value })} /></Field>
        <Field label="Subscription host"><Input value={panel.subscriptionHost} onChange={(e) => setPanel({ ...panel, subscriptionHost: e.target.value })} /></Field>
        <NumberField label="Enforcement interval (seconds)" value={panel.enforcementIntervalSeconds} onChange={(value) => setPanel({ ...panel, enforcementIntervalSeconds: value })} />
        <Field label="Adaptive tuning mode"><Select value={panel.adaptiveTuningMode} onChange={(e) => setPanel({ ...panel, adaptiveTuningMode: e.target.value })}><option value="off">Off</option><option value="recommend">Recommend only</option><option value="auto">Apply automatically</option></Select></Field>
        <NumberField label="Tuning interval (seconds)" value={panel.adaptiveTuningIntervalSeconds} onChange={(value) => setPanel({ ...panel, adaptiveTuningIntervalSeconds: value })} />
        <NumberField label="Tuning cooldown (seconds)" value={panel.adaptiveTuningCooldownSeconds} onChange={(value) => setPanel({ ...panel, adaptiveTuningCooldownSeconds: value })} />
        <NumberField label="Maximum Hysteria2 Mbps" value={panel.adaptiveTuningMaxHysteria2Mbps} onChange={(value) => setPanel({ ...panel, adaptiveTuningMaxHysteria2Mbps: value })} />
      </div>
      <Button variant="primary" icon={<Save size={16} />} onClick={() => onSave(panel)} disabled={busy}>Save panel settings</Button>
    </SettingsSection>

    <SettingsSection icon={<Terminal size={19} />} title="Runtime & observability" copy="Metrics, statistics, API, and connection limits." recommendation="Keep API and metrics listeners local unless remote access is protected with authentication and firewall rules." impact="Applies automatically" details="Metrics and statistics control runtime visibility. Limits protect the process from excessive connections and stalled sessions." defaultOpen>
      <div className="settings-grid settings-grid-3">
        <Switch checked={core.stats?.enabled ?? false} onChange={(enabled) => setCore({ ...core, stats: { enabled } })} label="Traffic statistics" />
        <Field label="Metrics listen address" hint="Empty disables metrics."><Input value={core.metricsAddr ?? ""} onChange={(e) => setCore({ ...core, metricsAddr: e.target.value || null })} /></Field>
      </div>
      <div className="settings-divider" />
      <Switch checked={core.api !== null} onChange={(enabled) => setCore({ ...core, api: enabled ? { listen: "127.0.0.1:62789", token: null, services: ["HandlerService", "StatsService"] } : null })} label="Enable management API" />
      {core.api ? <><div className="settings-grid"><Field label="API listen address"><Input value={core.api.listen} onChange={(e) => setCore({ ...core, api: { ...core.api!, listen: e.target.value } })} /></Field><Field label="Bearer token"><Input type="password" value={core.api.token ?? ""} onChange={(e) => setCore({ ...core, api: { ...core.api!, token: e.target.value || null } })} /></Field></div><ApiServicesEditor services={core.api.services} onChange={(services) => setCore({ ...core, api: { ...core.api!, services } })} /></> : null}
      <div className="settings-divider" />
      <div className="settings-grid settings-grid-3">{([['maxConnections','Process connections'],['maxConnectionsPerInbound','Per-inbound connections'],['maxConnectionsPerUser','Per-user connections'],['maxHandshakeSeconds','Handshake timeout (s)'],['maxIdleSeconds','Idle timeout (s)']] as const).map(([key, label]) => <Field key={key} label={label}><Input type="number" value={core.limits[key] ?? ""} placeholder="Unlimited" onChange={(e) => setCore({ ...core, limits: { ...core.limits, [key]: optionalNumber(e.target.value) } })} /></Field>)}</div>
    </SettingsSection>

    <SettingsSection icon={<Gauge size={19} />} title="Optimization" copy="Choose how much relay behavior Blackwire should decide for you." recommendation="Use Automatic unless a protocol needs the more conservative compatibility path. Open Custom only for measured troubleshooting or benchmarking." impact="Automatic · maintenance" details="Automatic uses Blackwire's tested Fast profile defaults without storing duplicate low-level values. Compatibility favors portable relay behavior. Existing specialized profiles remain available under Custom." defaultOpen>
      <OptimizationEditor value={core} onChange={setCore} />
    </SettingsSection>

    <SettingsSection icon={<Gauge size={19} />} title="QUIC, datagrams & FEC" copy="Global unreliable-traffic and lossy-network tuning." recommendation="Leave QUIC overrides off so Blackwire can size buffers and endpoint shards from available CPU and memory. Enable FEC mainly for measured lossy or mobile links." impact="Resource-aware · automatic" details="Automatic QUIC sizing is conservatively capped at four endpoint shards and 8 MB per socket buffer. Explicit values always win. Datagram policies choose unreliable lanes for eligible traffic; FEC adds recovery bandwidth.">
      <div className="settings-toggle-strip"><Switch checked={core.quic !== null} onChange={(enabled) => setCore({ ...core, quic: enabled ? defaultQuic : null })} label="QUIC socket overrides" /><Switch checked={core.datagram?.enabled ?? false} onChange={(enabled) => setCore({ ...core, datagram: enabled ? { ...(core.datagram ?? defaultDatagram), enabled: true } : null })} label="Datagram lane" /><Switch checked={core.fec !== null && core.fec.mode !== "off"} onChange={(enabled) => setCore({ ...core, fec: enabled ? { ...(core.fec ?? defaultFec), mode: core.fec?.mode === "off" ? "auto" : (core.fec?.mode ?? "auto") } : null })} label="Forward error correction" /></div>
      {core.quic ? <div className="settings-subsection"><h3>QUIC sockets</h3><div className="settings-grid settings-grid-3"><Switch checked={core.quic.reusePort} onChange={(reusePort) => setCore({ ...core, quic: { ...core.quic!, reusePort } })} label="Reuse UDP port" /><ScalarField label="Endpoint shards" value={core.quic.endpoints} keyword="cpu" onChange={(endpoints) => setCore({ ...core, quic: { ...core.quic!, endpoints } })} /><ScalarField label="Max datagram size" value={core.quic.maxDatagramSize} keyword="auto" onChange={(maxDatagramSize) => setCore({ ...core, quic: { ...core.quic!, maxDatagramSize } })} /><NumberField label="Receive buffer bytes" value={core.quic.recvBufferBytes} onChange={(recvBufferBytes) => setCore({ ...core, quic: { ...core.quic!, recvBufferBytes } })} /><NumberField label="Send buffer bytes" value={core.quic.sendBufferBytes} onChange={(sendBufferBytes) => setCore({ ...core, quic: { ...core.quic!, sendBufferBytes } })} /></div></div> : null}
      {core.datagram?.enabled ? <div className="settings-subsection"><h3>Datagram lane</h3><div className="settings-grid settings-grid-3"><Switch checked={core.datagram.udpOverDatagram} onChange={(udpOverDatagram) => setCore({ ...core, datagram: { ...core.datagram!, udpOverDatagram } })} label="UDP over datagrams" /><Field label="Policy"><Select value={core.datagram.policy} onChange={(e) => setCore({ ...core, datagram: { ...core.datagram!, policy: e.target.value as "standard" | "h2-plus" } })}><option value="standard">Standard</option><option value="h2-plus">H2+</option></Select></Field><NumberField label="Queue delay (ms)" value={core.datagram.maxQueueDelayMs} onChange={(maxQueueDelayMs) => setCore({ ...core, datagram: { ...core.datagram!, maxQueueDelayMs } })} /><Switch checked={core.datagram.fastDnsRetry} onChange={(fastDnsRetry) => setCore({ ...core, datagram: { ...core.datagram!, fastDnsRetry } })} label="Fast DNS retry" /><NumberField label="DNS retry delay (ms)" value={core.datagram.fastDnsRetryDelayMs} onChange={(fastDnsRetryDelayMs) => setCore({ ...core, datagram: { ...core.datagram!, fastDnsRetryDelayMs } })} /></div></div> : null}
      {core.fec && core.fec.mode !== "off" ? <FecEditor value={core.fec} onChange={(fec) => setCore({ ...core, fec })} /> : null}
    </SettingsSection>

    <div className="settings-core-save"><div><Shield size={18} /><span><strong>Core changes reload automatically.</strong><small>Blackwire validates and applies the new revision without a process restart.</small></span></div><Button variant="primary" icon={<Save size={16} />} onClick={() => onSaveCore(core)} disabled={busy}>Save Blackwire core</Button></div>
  </div>;
}

function SettingsSection({ icon, title, copy, recommendation, impact, details, defaultOpen = false, children }: { icon: React.ReactNode; title: string; copy: string; recommendation: string; impact: string; details: string; defaultOpen?: boolean; children: React.ReactNode }) {
  const [open, setOpen] = useState(defaultOpen);
  return <section className={`settings-section ${open ? "settings-section-open" : "settings-section-collapsed"}`}><button type="button" className="settings-section-head" aria-expanded={open} onClick={() => setOpen(!open)}><span className="settings-section-icon">{icon}</span><span className="settings-section-title"><span role="heading" aria-level={2}>{title}</span><small>{copy}</small></span><span className="settings-impact">{impact}</span><ChevronDown className="settings-chevron" size={18} /></button>{open ? <div className="settings-section-body"><div className="settings-guidance"><div><strong>Recommended</strong><p>{recommendation}</p></div><details><summary>Learn about these options</summary><p>{details}</p></details></div>{children}</div> : null}</section>;
}
function NumberField({ label, value, onChange }: { label: string; value: number; onChange: (value: number) => void }) { return <Field label={label}><Input type="number" min={0} value={value} onChange={(e) => onChange(Number(e.target.value))} /></Field>; }
function ScalarField({ label, value, keyword, onChange }: { label: string; value: number | string; keyword: string; onChange: (value: number | string) => void }) { return <Field label={label}><Input value={value} onChange={(e) => onChange(e.target.value === keyword ? keyword : Number(e.target.value))} /></Field>; }

function ApiServicesEditor({ services, onChange }: { services: string[]; onChange: (services: string[]) => void }) {
  const selected = services.length === 0 ? ["HandlerService", "StatsService"] : services;
  const toggle = (name: string, enabled: boolean) => {
    const next = enabled ? [...selected.filter((service) => service !== name), name] : selected.filter((service) => service !== name);
    if (next.some((service) => service === "HandlerService" || service === "StatsService")) onChange(next);
  };
  const enabledCount = ["HandlerService", "StatsService"].filter((service) => selected.includes(service)).length;
  return <div className="api-services"><div className="api-services-head"><div><h3>API capabilities</h3><p>Choose exactly what connected management clients can access.</p></div><span>{enabledCount} enabled</span></div><div className="api-service-list"><div className="api-service-row"><div><strong>Runtime management</strong><small>Add, remove, and inspect inbound or outbound handlers.</small></div><Switch checked={selected.includes("HandlerService")} onChange={(enabled) => toggle("HandlerService", enabled)} label="Handler service" /></div><div className="api-service-row"><div><strong>Traffic statistics</strong><small>Read transfer counters and runtime usage measurements.</small></div><Switch checked={selected.includes("StatsService")} onChange={(enabled) => toggle("StatsService", enabled)} label="Statistics service" /></div></div><p className="api-services-empty">Keep at least one capability enabled, or turn off the management API.</p></div>;
}

const optimizationModes: Array<{ mode: OptimizationMode; title: string; description: string }> = [
  { mode: "automatic", title: "Automatic", description: "Use tested Fast defaults with adaptive relay paths and safe fallbacks." },
  { mode: "compatibility", title: "Compatibility", description: "Favor portable behavior when a client, transport, or host is sensitive." },
  { mode: "custom", title: "Custom", description: "Keep specialized profiles and low-level overrides under your control." }
];

function OptimizationEditor({ value, onChange }: { value: CoreSettings; onChange: (value: CoreSettings) => void }) {
  const mode = optimizationModeFromSettings(value);
  const [expertOpen, setExpertOpen] = useState(mode === "custom");
  useEffect(() => {
    if (mode === "custom") setExpertOpen(true);
  }, [mode]);

  const selectMode = (next: OptimizationMode) => {
    onChange(applyOptimizationMode(value, next));
    setExpertOpen(next === "custom");
  };

  const status = optimizationStatusFromSettings(value);

  return <div className="optimization-editor">
    <div className="optimization-mode-list" role="radiogroup" aria-label="Optimization mode">
      {optimizationModes.map((option) => <button
        key={option.mode}
        type="button"
        role="radio"
        aria-checked={mode === option.mode}
        className={`optimization-mode ${mode === option.mode ? "optimization-mode-active" : ""}`}
        onClick={() => selectMode(option.mode)}
      >
        <span className="optimization-mode-indicator" aria-hidden="true" />
        <span><strong>{option.title}</strong><small>{option.description}</small></span>
      </button>)}
    </div>

    <div className="optimization-status" aria-live="polite">
      <span>{status.label}</span>
      <p>{status.detail}</p>
    </div>

    <details className="optimization-expert" open={expertOpen} onToggle={(event) => setExpertOpen(event.currentTarget.open)}>
      <summary><span>Expert overrides</span><small>Profiles, relay internals, Vision, and first-packet controls</small></summary>
      <div className="optimization-expert-body">
        <Field label="Runtime profile"><Select value={value.profile} onChange={(e) => onChange({ ...value, profile: e.target.value as CoreSettings["profile"] })}>{["compat", "fast", "latency", "throughput", "badnet", "mobile", "stealth"].map((profile) => <option key={profile}>{profile}</option>)}</Select></Field>
        <div className="settings-toggle-strip"><Switch checked={value.fast !== null} onChange={(enabled) => onChange({ ...value, fast: enabled ? defaultFastSettings : null })} label="Fast-path overrides" /><Switch checked={value.budget !== null} onChange={(enabled) => onChange({ ...value, budget: enabled ? defaultBudget : null })} label="Performance budget override" /><Switch checked={value.vision !== null} onChange={(enabled) => onChange({ ...value, vision: enabled ? defaultVision : null })} label="Vision override" /><Switch checked={value.firstPacketBoost?.enabled ?? false} onChange={(enabled) => onChange({ ...value, firstPacketBoost: enabled ? { ...(value.firstPacketBoost ?? defaultBoost), enabled: true } : null })} label="First-packet boost" /></div>
        {value.fast ? <FastEditor value={value.fast} onChange={(fast) => onChange({ ...value, fast })} /> : null}
        {value.budget ? <BudgetEditor value={value.budget} onChange={(budget) => onChange({ ...value, budget })} /> : null}
        {value.vision ? <div className="settings-subsection"><h3>Vision optimization</h3><div className="settings-grid settings-grid-3"><Field label="Direct-copy policy"><Select value={value.vision.directCopy} onChange={(e) => onChange({ ...value, vision: { ...value.vision!, directCopy: e.target.value as typeof value.vision.directCopy } })}>{["auto", "disabled", "require"].map((policy) => <option key={policy}>{policy}</option>)}</Select></Field><NumberField label="Packets to filter" value={value.vision.maxPacketsToFilter} onChange={(maxPacketsToFilter) => onChange({ ...value, vision: { ...value.vision!, maxPacketsToFilter } })} /><Switch checked={value.vision.allowSpliceAfterDirect} onChange={(allowSpliceAfterDirect) => onChange({ ...value, vision: { ...value.vision!, allowSpliceAfterDirect } })} label="Allow splice after direct copy" /></div></div> : null}
        {value.firstPacketBoost ? <BoostEditor value={value.firstPacketBoost} onChange={(firstPacketBoost) => onChange({ ...value, firstPacketBoost })} /> : null}
      </div>
    </details>
  </div>;
}

function FastEditor({ value, onChange }: { value: NonNullable<CoreSettings["fast"]>; onChange: (value: NonNullable<CoreSettings["fast"]>) => void }) {
  return <div className="settings-subsection"><h3>Fast-path relay</h3><div className="settings-grid settings-grid-3"><Switch checked={value.strictProduction} onChange={(strictProduction) => onChange({ ...value, strictProduction })} label="Strict production mode" /><EnumField label="Pool policy" value={value.pool} options={["adaptive", "disabled", "fixed"]} onChange={(pool) => onChange({ ...value, pool: pool as typeof value.pool })} /><EnumField label="Splice policy" value={value.splice} options={["adaptive", "disabled", "always"]} onChange={(splice) => onChange({ ...value, splice: splice as typeof value.splice })} /><Field label="Relay engine" hint="Keep Legacy only as a temporary troubleshooting fallback."><Select value={value.relay.engine} onChange={(e) => onChange({ ...value, relay: { ...value.relay, engine: e.target.value as typeof value.relay.engine } })}><option value="v2">V2 — recommended</option><option value="legacy">Legacy — troubleshooting only</option></Select></Field>{value.relay.engine === "v2" ? <><EnumField label="Flush policy" value={value.relay.flush} options={["immediate", "deferred", "adaptive"]} onChange={(flush) => onChange({ ...value, relay: { ...value.relay, flush: flush as typeof value.relay.flush } })} /><NumberField label="Initial relay buffer" value={value.relay.initialBuffer} onChange={(initialBuffer) => onChange({ ...value, relay: { ...value.relay, initialBuffer } })} /><NumberField label="Maximum relay buffer" value={value.relay.maxBuffer} onChange={(maxBuffer) => onChange({ ...value, relay: { ...value.relay, maxBuffer } })} /></> : null}<EnumField label="Linux zero-copy" value={value.linux.zerocopy} options={["disabled", "bulk", "always"]} onChange={(zerocopy) => onChange({ ...value, linux: { ...value.linux, zerocopy: zerocopy as typeof value.linux.zerocopy } })} /><NumberField label="Zero-copy minimum bytes" value={value.linux.zerocopyMinBytes} onChange={(zerocopyMinBytes) => onChange({ ...value, linux: { ...value.linux, zerocopyMinBytes } })} /><EnumField label="io_uring" value={value.linux.ioUring} options={["disabled", "auto", "require"]} onChange={(ioUring) => onChange({ ...value, linux: { ...value.linux, ioUring: ioUring as typeof value.linux.ioUring } })} /></div></div>;
}

function BudgetEditor({ value, onChange }: { value: NonNullable<CoreSettings["budget"]>; onChange: (value: NonNullable<CoreSettings["budget"]>) => void }) {
  return <div className="settings-subsection"><h3>Performance budget</h3><div className="settings-grid settings-grid-3"><NumberField label="Maximum protocol layers" value={value.maxProtocolLayers} onChange={(maxProtocolLayers) => onChange({ ...value, maxProtocolLayers })} /><NumberField label="Maximum route rules" value={value.maxRouteRules} onChange={(maxRouteRules) => onChange({ ...value, maxRouteRules })} /><Switch checked={value.allowSniffing} onChange={(allowSniffing) => onChange({ ...value, allowSniffing })} label="Allow sniffing" /><Switch checked={value.preferDirectCopy} onChange={(preferDirectCopy) => onChange({ ...value, preferDirectCopy })} label="Prefer direct copy" /></div></div>;
}

function BoostEditor({ value, onChange }: { value: NonNullable<CoreSettings["firstPacketBoost"]>; onChange: (value: NonNullable<CoreSettings["firstPacketBoost"]>) => void }) {
  return <div className="settings-subsection"><h3>First-packet acceleration</h3><div className="settings-grid settings-grid-3"><Switch checked={value.dns} onChange={(dns) => onChange({ ...value, dns })} label="Pre-resolve DNS" /><Switch checked={value.sendEarlyPayload} onChange={(sendEarlyPayload) => onChange({ ...value, sendEarlyPayload })} label="Send early payload" /></div></div>;
}

function EnumField({ label, value, options, onChange }: { label: string; value: string; options: string[]; onChange: (value: string) => void }) { return <Field label={label}><Select value={value} onChange={(e) => onChange(e.target.value)}>{options.map((option) => <option key={option}>{option}</option>)}</Select></Field>; }

function FecEditor({ value, onChange }: { value: NonNullable<CoreSettings["fec"]>; onChange: (value: NonNullable<CoreSettings["fec"]>) => void }) {
  return <div className="settings-subsection"><h3>Forward error correction</h3><div className="settings-grid settings-grid-3"><Field label="Mode"><Select value={value.mode} onChange={(e) => onChange({ ...value, mode: e.target.value as typeof value.mode })}>{["auto", "xor1-of-n", "reed-solomon", "raptor-like"].map((mode) => <option key={mode}>{mode}</option>)}</Select></Field><NumberField label="Maximum overhead %" value={value.maxOverheadPercent} onChange={(maxOverheadPercent) => onChange({ ...value, maxOverheadPercent })} /><Field label="Protected packet classes"><Textarea rows={3} value={value.protectClasses.join("\n")} onChange={(e) => onChange({ ...value, protectClasses: lines(e.target.value) })} /></Field><Switch checked={value.avoidBulkTcp} onChange={(avoidBulkTcp) => onChange({ ...value, avoidBulkTcp })} label="Avoid bulk TCP" /><Switch checked={value.disableForSequentialDns} onChange={(disableForSequentialDns) => onChange({ ...value, disableForSequentialDns })} label="Skip sequential DNS" />{([['minConcurrencyForBlockFec','Minimum concurrency'],['maxGenerationPackets','Generation packets'],['maxGenerationDelayMs','Generation delay (ms)'],['recoveryDeadlineMs','Recovery deadline (ms)'],['dedupWindowPackets','Dedup window']] as const).map(([key, label]) => <NumberField key={key} label={label} value={value[key]} onChange={(next) => onChange({ ...value, [key]: next })} />)}</div></div>;
}
