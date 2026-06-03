import { Plus, Save, Trash2, Wand2 } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { Badge } from "../components/atoms/Badge";
import { Button } from "../components/atoms/Button";
import { Input, Select, Textarea } from "../components/atoms/Input";
import { Switch } from "../components/atoms/Switch";
import { Field } from "../components/molecules/Field";
import {
  applyAdaptiveRoutingTemplate,
  buildSectionValue,
  createSectionEditorState,
  isStructuredSection,
  replaceSectionJson,
  syncSectionState,
  validateSectionState,
  type AdvancedConfigEditorState,
  type DnsHostEditor,
  type DnsServerEditor,
  type RoutingBalancerEditor,
  type RoutingBalancerProfileEditor,
  type RoutingRuleEditor
} from "../lib/advancedConfigConfigurator";
import type { CapabilityMap, ConfigSection, Outbound } from "../lib/types";

const profileOptions = ["compat", "fast"];
const routingStrategies = ["adaptive", "random", "roundRobin", "leastPing", "leastLoad"];
const dnsQueryStrategies = ["", "UseIP", "UseIPv4", "UseIPv6", "UseSystem"];
const fastPoolOptions = ["", "disabled", "auto", "always"];
const fastSpliceOptions = ["", "disabled", "adaptive", "always"];

export function SectionsPage({
  sections,
  capabilities,
  outbounds,
  busy,
  onSave
}: {
  sections: ConfigSection[];
  capabilities: CapabilityMap | null;
  outbounds: Outbound[];
  busy: boolean;
  onSave: (name: string, enabled: boolean, value: string) => void;
}) {
  const [selectedName, setSelectedName] = useState("");
  const [editor, setEditor] = useState<AdvancedConfigEditorState | null>(null);
  const notes = new Map((capabilities?.config ?? []).map((item) => [item.key, item]));
  const selectedSection = sections.find((section) => section.name === selectedName) ?? null;

  useEffect(() => {
    if (!selectedName && sections.length > 0) setSelectedName(sections[0].name);
    else if (selectedName && !sections.some((section) => section.name === selectedName) && sections.length > 0) {
      setSelectedName(sections[0].name);
    }
  }, [sections, selectedName]);

  useEffect(() => {
    setEditor(createSectionEditorState(selectedSection));
  }, [selectedSection]);

  const validationIssues = useMemo(() => (editor ? validateSectionState(editor) : []), [editor]);
  const adaptiveTemplateAvailable = selectedName === "routing" && outbounds.filter((outbound) => outbound.enabled).length >= 2;
  const saveDisabled = busy || !editor || validationIssues.length > 0;

  const updateEditor = (next: AdvancedConfigEditorState) => setEditor(syncSectionState(next));

  const save = () => {
    if (!editor || saveDisabled) return;
    onSave(editor.name, editor.enabled, buildSectionValue(editor));
  };

  return (
    <div className="page">
      <div className="page-title">
        <h1>Advanced Config</h1>
        <p>Advanced top-level Blackwire JSON for routing, DNS, TUN, profiles, QUIC/datagram/FEC, and runtime controls. Everyday changes still belong in Users, Inbounds, Outbounds, and Settings.</p>
      </div>
      <div className="two-column advanced-config-layout">
        <section className="work-panel">
          <div className="panel-toolbar">
            <h2>Available sections</h2>
          </div>
          <div className="stack-list">
            {sections.map((section) => {
              const cap = notes.get(section.name);
              const active = section.name === selectedName;
              return (
                <button className={`stack-row ${active ? "stack-row-active" : ""}`} key={section.name} onClick={() => setSelectedName(section.name)} type="button">
                  <span>
                    <strong>{section.name}</strong>
                    <small>{cap?.notes ?? "Blackwire native config section"}</small>
                  </span>
                  <Badge tone={section.enabled ? "green" : "gray"}>{isStructuredSection(section.name) ? "structured" : "json"}</Badge>
                </button>
              );
            })}
          </div>
        </section>

        <section className="work-panel editor-panel">
          {editor ? (
            <>
              <div className="section-editor-head">
                <div>
                  <h2>{editor.name}</h2>
                  <p className="field-hint">
                    {isStructuredSection(editor.name)
                      ? "Common fields are structured here. Advanced JSON stays available as an escape hatch."
                      : "This section still uses raw JSON in this pass."}
                  </p>
                </div>
                {editor.name === "routing" ? (
                  <Button variant="secondary" icon={<Wand2 size={16} />} onClick={() => setEditor((current) => (current ? applyAdaptiveRoutingTemplate(current, outbounds) : current))} disabled={busy || !adaptiveTemplateAvailable}>
                    Adaptive Template
                  </Button>
                ) : null}
              </div>

              <section className="drawer-card">
                <div className="summary-head">
                  <div>
                    <strong>{editor.name}</strong>
                    <span>{isStructuredSection(editor.name) ? "Structured editor with advanced fallback" : "Raw JSON editor"}</span>
                  </div>
                  <Switch checked={editor.enabled} onChange={(enabled) => setEditor((current) => (current ? { ...current, enabled } : current))} label={editor.enabled ? "Enabled" : "Disabled"} />
                </div>
                <div className="summary-badges">
                  <span className="summary-chip">{isStructuredSection(editor.name) ? "Structured" : "Raw JSON"}</span>
                  <span className="summary-chip summary-chip-soft">{editor.enabled ? "Included in generated config" : "Excluded from generated config"}</span>
                </div>
              </section>

              {isStructuredSection(editor.name) ? (
                <StructuredSectionEditor editor={editor} onChange={updateEditor} />
              ) : (
                <section className="drawer-card">
                  <Field label="Section JSON">
                    <div className="advanced-slice">
                      <Textarea rows={18} value={editor.rawText} onChange={(e) => setEditor((current) => (current ? replaceSectionJson(current, e.target.value) : current))} />
                      {editor.rawError ? <div className="field-error">{editor.rawError}</div> : null}
                    </div>
                  </Field>
                </section>
              )}

              {isStructuredSection(editor.name) ? (
                <section className="drawer-card">
                  <div className="section-editor-head">
                    <div>
                      <h3>Advanced JSON</h3>
                      <p>Unknown keys are preserved. Use this only for section fields that do not have a dedicated control yet.</p>
                    </div>
                    <Button variant="ghost" onClick={() => setEditor((current) => (current ? { ...current, advancedOpen: !current.advancedOpen } : current))}>
                      {editor.advancedOpen ? "Hide JSON" : "Show JSON"}
                    </Button>
                  </div>
                  {editor.advancedOpen ? (
                    <Field label="Section JSON">
                      <div className="advanced-slice">
                        <Textarea rows={14} value={editor.rawText} onChange={(e) => setEditor((current) => (current ? replaceSectionJson(current, e.target.value) : current))} />
                        {editor.rawError ? <div className="field-error">{editor.rawError}</div> : null}
                      </div>
                    </Field>
                  ) : (
                    <p className="field-hint">Advanced JSON is collapsed by default so the structured editor stays readable.</p>
                  )}
                </section>
              ) : null}

              {validationIssues.length > 0 ? <div className="error-line">{validationIssues[0].message}</div> : null}

              <Button variant="primary" icon={<Save size={16} />} onClick={save} disabled={saveDisabled} loading={busy}>
                Save Advanced Config
              </Button>
            </>
          ) : (
            <div className="empty">Select a section to edit.</div>
          )}
        </section>
      </div>
    </div>
  );
}

function StructuredSectionEditor({
  editor,
  onChange
}: {
  editor: AdvancedConfigEditorState;
  onChange: (next: AdvancedConfigEditorState) => void;
}) {
  if (editor.name === "routing") return <RoutingEditor editor={editor} onChange={onChange} />;
  if (editor.name === "dns") return <DnsEditor editor={editor} onChange={onChange} />;
  if (editor.name === "tun") return <TunEditor editor={editor} onChange={onChange} />;
  if (editor.name === "api") return <ApiEditor editor={editor} onChange={onChange} />;
  if (editor.name === "metricsAddr") return <MetricsEditor editor={editor} onChange={onChange} />;
  if (editor.name === "profile") return <ProfileEditor editor={editor} onChange={onChange} />;
  if (editor.name === "fast") return <FastEditor editor={editor} onChange={onChange} />;
  return null;
}

function RoutingEditor({ editor, onChange }: { editor: AdvancedConfigEditorState; onChange: (next: AdvancedConfigEditorState) => void }) {
  const setRule = (index: number, patch: Partial<RoutingRuleEditor>) =>
    onChange({
      ...editor,
      routingRules: editor.routingRules.map((rule, current) => (current === index ? { ...rule, ...patch } : rule))
    });

  const setBalancer = (index: number, patch: Partial<RoutingBalancerEditor>) =>
    onChange({
      ...editor,
      routingBalancers: editor.routingBalancers.map((balancer, current) => (current === index ? { ...balancer, ...patch } : balancer))
    });

  const setProfile = (balancerIndex: number, profileIndex: number, patch: Partial<RoutingBalancerProfileEditor>) =>
    onChange({
      ...editor,
      routingBalancers: editor.routingBalancers.map((balancer, current) =>
        current === balancerIndex
          ? {
              ...balancer,
              profiles: balancer.profiles.map((profile, profileCurrent) => (profileCurrent === profileIndex ? { ...profile, ...patch } : profile))
            }
          : balancer
      )
    });

  return (
    <>
      <section className="drawer-card configurator-section">
        <div className="section-editor-head">
          <div>
            <h3>Routing basics</h3>
            <p>Common defaults and matching rules for where traffic should go.</p>
          </div>
        </div>
        <Field label="Domain strategy" hint="Leave empty to keep the runtime default.">
          <Input value={editor.routingDomainStrategy} onChange={(e) => onChange({ ...editor, routingDomainStrategy: e.target.value })} placeholder="AsIs" />
        </Field>
      </section>

      <section className="drawer-card configurator-section">
        <div className="section-editor-head">
          <div>
            <h3>Rules</h3>
            <p>Add common field-based routing rules without dropping into raw JSON.</p>
          </div>
          <Button
            variant="secondary"
            icon={<Plus size={16} />}
            onClick={() =>
              onChange({
                ...editor,
                routingRules: [...editor.routingRules, { type: "field", domain: "", ip: "", port: "", network: "", sourceIP: "", inboundTag: "", protocol: "", user: "", outboundTag: "", balancerTag: "" }]
              })
            }
          >
            Add Rule
          </Button>
        </div>
        {editor.routingRules.length === 0 ? <p className="field-hint">No rules yet. Add one or rely on the fallback defaults in advanced JSON.</p> : null}
        {editor.routingRules.map((rule, index) => (
          <div className="drawer-card advanced-config-subcard" key={`rule-${index}`}>
            <div className="section-editor-head">
              <h3>Rule {index + 1}</h3>
              <Button variant="ghost" icon={<Trash2 size={16} />} onClick={() => onChange({ ...editor, routingRules: editor.routingRules.filter((_, current) => current !== index) })}>
                Remove
              </Button>
            </div>
            <div className="configurator-grid">
              <Field label="Type">
                <Input value={rule.type} onChange={(e) => setRule(index, { type: e.target.value })} />
              </Field>
              <Field label="Outbound tag">
                <Input value={rule.outboundTag} onChange={(e) => setRule(index, { outboundTag: e.target.value })} placeholder="freedom" />
              </Field>
              <Field label="Balancer tag">
                <Input value={rule.balancerTag} onChange={(e) => setRule(index, { balancerTag: e.target.value })} placeholder="auto-proxy" />
              </Field>
              <Field label="Port">
                <Input value={rule.port} onChange={(e) => setRule(index, { port: e.target.value })} placeholder="80,443" />
              </Field>
              <Field label="Domain CSV">
                <Input value={rule.domain} onChange={(e) => setRule(index, { domain: e.target.value })} placeholder="geosite:google, example.com" />
              </Field>
              <Field label="IP CSV">
                <Input value={rule.ip} onChange={(e) => setRule(index, { ip: e.target.value })} placeholder="geoip:private, 1.1.1.1" />
              </Field>
              <Field label="Network">
                <Input value={rule.network} onChange={(e) => setRule(index, { network: e.target.value })} placeholder="tcp,udp" />
              </Field>
              <Field label="Source IP CSV">
                <Input value={rule.sourceIP} onChange={(e) => setRule(index, { sourceIP: e.target.value })} placeholder="192.168.1.0/24" />
              </Field>
              <Field label="Inbound tag CSV">
                <Input value={rule.inboundTag} onChange={(e) => setRule(index, { inboundTag: e.target.value })} placeholder="vless-main" />
              </Field>
              <Field label="Protocol CSV">
                <Input value={rule.protocol} onChange={(e) => setRule(index, { protocol: e.target.value })} placeholder="http,tls,quic" />
              </Field>
              <Field label="User CSV">
                <Input value={rule.user} onChange={(e) => setRule(index, { user: e.target.value })} placeholder="alice@example.com" />
              </Field>
            </div>
          </div>
        ))}
      </section>

      <section className="drawer-card configurator-section">
        <div className="section-editor-head">
          <div>
            <h3>Balancers</h3>
            <p>Define reusable balancers and health checks for outbound selection.</p>
          </div>
          <Button
            variant="secondary"
            icon={<Plus size={16} />}
            onClick={() =>
              onChange({
                ...editor,
                routingBalancers: [
                  ...editor.routingBalancers,
                  {
                    tag: "",
                    selector: "",
                    strategy: "",
                    fallbackTag: "",
                    adaptiveFailureThreshold: "",
                    adaptiveCooldownSecs: "",
                    adaptiveEwmaAlpha: "",
                    adaptiveSwitchMargin: "",
                    healthUrl: "",
                    healthIntervalSecs: "",
                    healthTimeoutSecs: "",
                    healthMaxFailures: "",
                    profiles: []
                  }
                ]
              })
            }
          >
            Add Balancer
          </Button>
        </div>
        {editor.routingBalancers.length === 0 ? <p className="field-hint">No balancers defined. Add one for adaptive or load-based routing.</p> : null}
        {editor.routingBalancers.map((balancer, index) => (
          <div className="drawer-card advanced-config-subcard" key={`balancer-${index}`}>
            <div className="section-editor-head">
              <h3>Balancer {index + 1}</h3>
              <Button variant="ghost" icon={<Trash2 size={16} />} onClick={() => onChange({ ...editor, routingBalancers: editor.routingBalancers.filter((_, current) => current !== index) })}>
                Remove
              </Button>
            </div>
            <div className="configurator-grid">
              <Field label="Tag">
                <Input value={balancer.tag} onChange={(e) => setBalancer(index, { tag: e.target.value })} placeholder="auto-proxy" />
              </Field>
              <Field label="Selector CSV">
                <Input value={balancer.selector} onChange={(e) => setBalancer(index, { selector: e.target.value })} placeholder="proxy-a, proxy-b" />
              </Field>
              <Field label="Strategy">
                <Select value={balancer.strategy} onChange={(e) => setBalancer(index, { strategy: e.target.value })}>
                  {routingStrategies.map((item) => (
                    <option key={item} value={item}>
                      {item || "default"}
                    </option>
                  ))}
                </Select>
              </Field>
              <Field label="Fallback tag">
                <Input value={balancer.fallbackTag} onChange={(e) => setBalancer(index, { fallbackTag: e.target.value })} placeholder="freedom" />
              </Field>
              <Field label="Failure threshold">
                <Input value={balancer.adaptiveFailureThreshold} onChange={(e) => setBalancer(index, { adaptiveFailureThreshold: e.target.value })} placeholder="2" />
              </Field>
              <Field label="Cooldown seconds">
                <Input value={balancer.adaptiveCooldownSecs} onChange={(e) => setBalancer(index, { adaptiveCooldownSecs: e.target.value })} placeholder="30" />
              </Field>
              <Field label="EWMA alpha">
                <Input value={balancer.adaptiveEwmaAlpha} onChange={(e) => setBalancer(index, { adaptiveEwmaAlpha: e.target.value })} placeholder="0.2" />
              </Field>
              <Field label="Switch margin">
                <Input value={balancer.adaptiveSwitchMargin} onChange={(e) => setBalancer(index, { adaptiveSwitchMargin: e.target.value })} placeholder="0.15" />
              </Field>
              <Field label="Health URL">
                <Input value={balancer.healthUrl} onChange={(e) => setBalancer(index, { healthUrl: e.target.value })} placeholder="http://www.gstatic.com/generate_204" />
              </Field>
              <Field label="Health interval seconds">
                <Input value={balancer.healthIntervalSecs} onChange={(e) => setBalancer(index, { healthIntervalSecs: e.target.value })} placeholder="30" />
              </Field>
              <Field label="Health timeout seconds">
                <Input value={balancer.healthTimeoutSecs} onChange={(e) => setBalancer(index, { healthTimeoutSecs: e.target.value })} placeholder="5" />
              </Field>
              <Field label="Health max failures">
                <Input value={balancer.healthMaxFailures} onChange={(e) => setBalancer(index, { healthMaxFailures: e.target.value })} placeholder="2" />
              </Field>
            </div>
            <div className="section-editor-head">
              <div>
                <h3>Profiles</h3>
                <p>Optional adaptive profile hints for this balancer.</p>
              </div>
              <Button
                variant="secondary"
                icon={<Plus size={16} />}
                onClick={() =>
                  onChange({
                    ...editor,
                    routingBalancers: editor.routingBalancers.map((item, current) =>
                      current === index ? { ...item, profiles: [...item.profiles, { name: "", outboundTag: "" }] } : item
                    )
                  })
                }
              >
                Add Profile
              </Button>
            </div>
            {balancer.profiles.map((profile, profileIndex) => (
              <div className="configurator-grid advanced-config-inline-row" key={`profile-${profileIndex}`}>
                <Field label={`Profile ${profileIndex + 1} name`}>
                  <Input value={profile.name} onChange={(e) => setProfile(index, profileIndex, { name: e.target.value })} placeholder="stable" />
                </Field>
                <Field label="Outbound tag">
                  <Input value={profile.outboundTag} onChange={(e) => setProfile(index, profileIndex, { outboundTag: e.target.value })} placeholder="proxy-a" />
                </Field>
                <Button
                  variant="ghost"
                  icon={<Trash2 size={16} />}
                  onClick={() =>
                    onChange({
                      ...editor,
                      routingBalancers: editor.routingBalancers.map((item, current) =>
                        current === index ? { ...item, profiles: item.profiles.filter((_, currentProfile) => currentProfile !== profileIndex) } : item
                      )
                    })
                  }
                >
                  Remove
                </Button>
              </div>
            ))}
          </div>
        ))}
      </section>
    </>
  );
}

function DnsEditor({ editor, onChange }: { editor: AdvancedConfigEditorState; onChange: (next: AdvancedConfigEditorState) => void }) {
  const setServer = (index: number, patch: Partial<DnsServerEditor>) =>
    onChange({
      ...editor,
      dnsServers: editor.dnsServers.map((server, current) => (current === index ? { ...server, ...patch } : server))
    });
  const setHost = (index: number, patch: Partial<DnsHostEditor>) =>
    onChange({
      ...editor,
      dnsHosts: editor.dnsHosts.map((host, current) => (current === index ? { ...host, ...patch } : host))
    });

  return (
    <>
      <section className="drawer-card configurator-section">
        <div className="section-editor-head">
          <div>
            <h3>DNS basics</h3>
            <p>Set top-level strategy, cache behavior, and fallback handling here.</p>
          </div>
        </div>
        <div className="configurator-grid">
          <Field label="Query strategy">
            <Select value={editor.dnsQueryStrategy} onChange={(e) => onChange({ ...editor, dnsQueryStrategy: e.target.value })}>
              {dnsQueryStrategies.map((item) => (
                <option key={item} value={item}>
                  {item || "default"}
                </option>
              ))}
            </Select>
          </Field>
          <Field label="Client IP">
            <Input value={editor.dnsClientIp} onChange={(e) => onChange({ ...editor, dnsClientIp: e.target.value })} placeholder="1.1.1.1" />
          </Field>
          <Switch checked={editor.dnsDisableCache} onChange={(dnsDisableCache) => onChange({ ...editor, dnsDisableCache })} label="Disable cache" />
          <Switch checked={editor.dnsDisableFallback} onChange={(dnsDisableFallback) => onChange({ ...editor, dnsDisableFallback })} label="Disable fallback" />
          <Switch checked={editor.dnsDisableFallbackIfMatch} onChange={(dnsDisableFallbackIfMatch) => onChange({ ...editor, dnsDisableFallbackIfMatch })} label="Disable fallback if match" />
          <Switch checked={editor.dnsEnableParallelQuery} onChange={(dnsEnableParallelQuery) => onChange({ ...editor, dnsEnableParallelQuery })} label="Enable parallel query" />
          <Switch checked={editor.dnsUseSystemHosts} onChange={(dnsUseSystemHosts) => onChange({ ...editor, dnsUseSystemHosts })} label="Use system hosts" />
          <Switch checked={editor.dnsServeStale} onChange={(dnsServeStale) => onChange({ ...editor, dnsServeStale })} label="Serve stale" />
          <Field label="Serve expired TTL">
            <Input value={editor.dnsServeExpiredTTL} onChange={(e) => onChange({ ...editor, dnsServeExpiredTTL: e.target.value })} placeholder="0" />
          </Field>
        </div>
      </section>

      <section className="drawer-card configurator-section">
        <div className="section-editor-head">
          <div>
            <h3>Servers</h3>
            <p>Use simple string entries or richer object entries depending on how much control you need.</p>
          </div>
          <div className="button-row">
            <Button
              variant="secondary"
              icon={<Plus size={16} />}
              onClick={() =>
                onChange({
                  ...editor,
                  dnsServers: [
                    ...editor.dnsServers,
                    { mode: "string", value: "", address: "", port: "", domains: "", expectedIPs: "", tag: "", clientIP: "", queryStrategy: "", skipFallback: false, finalQuery: false, disableCache: false, timeoutMs: "", serveStale: false, serveExpiredTTL: "" }
                  ]
                })
              }
            >
              Add String Server
            </Button>
            <Button
              variant="secondary"
              icon={<Plus size={16} />}
              onClick={() =>
                onChange({
                  ...editor,
                  dnsServers: [
                    ...editor.dnsServers,
                    { mode: "object", value: "", address: "", port: "", domains: "", expectedIPs: "", tag: "", clientIP: "", queryStrategy: "", skipFallback: false, finalQuery: false, disableCache: false, timeoutMs: "4000", serveStale: false, serveExpiredTTL: "" }
                  ]
                })
              }
            >
              Add Object Server
            </Button>
          </div>
        </div>
        {editor.dnsServers.length === 0 ? <p className="field-hint">No servers defined yet.</p> : null}
        {editor.dnsServers.map((server, index) => (
          <div className="drawer-card advanced-config-subcard" key={`dns-server-${index}`}>
            <div className="section-editor-head">
              <h3>Server {index + 1}</h3>
              <div className="button-row">
                <Button variant="ghost" onClick={() => setServer(index, { mode: server.mode === "string" ? "object" : "string" })}>
                  {server.mode === "string" ? "Use Object" : "Use String"}
                </Button>
                <Button variant="ghost" icon={<Trash2 size={16} />} onClick={() => onChange({ ...editor, dnsServers: editor.dnsServers.filter((_, current) => current !== index) })}>
                  Remove
                </Button>
              </div>
            </div>
            {server.mode === "string" ? (
              <Field label="Server value">
                <Input value={server.value} onChange={(e) => setServer(index, { value: e.target.value })} placeholder="1.1.1.1" />
              </Field>
            ) : (
              <div className="configurator-grid">
                <Field label="Address">
                  <Input value={server.address} onChange={(e) => setServer(index, { address: e.target.value })} placeholder="https://1.1.1.1/dns-query" />
                </Field>
                <Field label="Port">
                  <Input value={server.port} onChange={(e) => setServer(index, { port: e.target.value })} placeholder="53" />
                </Field>
                <Field label="Domains CSV">
                  <Input value={server.domains} onChange={(e) => setServer(index, { domains: e.target.value })} placeholder="geosite:google" />
                </Field>
                <Field label="Expected IPs CSV">
                  <Input value={server.expectedIPs} onChange={(e) => setServer(index, { expectedIPs: e.target.value })} placeholder="1.1.1.1, 1.0.0.1" />
                </Field>
                <Field label="Tag">
                  <Input value={server.tag} onChange={(e) => setServer(index, { tag: e.target.value })} placeholder="cloudflare" />
                </Field>
                <Field label="Client IP">
                  <Input value={server.clientIP} onChange={(e) => setServer(index, { clientIP: e.target.value })} placeholder="1.1.1.1" />
                </Field>
                <Field label="Query strategy">
                  <Select value={server.queryStrategy} onChange={(e) => setServer(index, { queryStrategy: e.target.value })}>
                    {dnsQueryStrategies.map((item) => (
                      <option key={item} value={item}>
                        {item || "default"}
                      </option>
                    ))}
                  </Select>
                </Field>
                <Field label="Timeout ms">
                  <Input value={server.timeoutMs} onChange={(e) => setServer(index, { timeoutMs: e.target.value })} placeholder="4000" />
                </Field>
                <Switch checked={server.skipFallback} onChange={(skipFallback) => setServer(index, { skipFallback })} label="Skip fallback" />
                <Switch checked={server.finalQuery} onChange={(finalQuery) => setServer(index, { finalQuery })} label="Final query" />
                <Switch checked={server.disableCache} onChange={(disableCache) => setServer(index, { disableCache })} label="Disable cache" />
                <Switch checked={server.serveStale} onChange={(serveStale) => setServer(index, { serveStale })} label="Serve stale" />
                <Field label="Serve expired TTL">
                  <Input value={server.serveExpiredTTL} onChange={(e) => setServer(index, { serveExpiredTTL: e.target.value })} placeholder="0" />
                </Field>
              </div>
            )}
          </div>
        ))}
      </section>

      <section className="drawer-card configurator-section">
        <div className="section-editor-head">
          <div>
            <h3>Hosts</h3>
            <p>Map domains to one or many fixed IP or hostname values.</p>
          </div>
          <Button
            variant="secondary"
            icon={<Plus size={16} />}
            onClick={() => onChange({ ...editor, dnsHosts: [...editor.dnsHosts, { domain: "", values: "" }] })}
          >
            Add Host
          </Button>
        </div>
        {editor.dnsHosts.map((host, index) => (
          <div className="configurator-grid advanced-config-inline-row" key={`dns-host-${index}`}>
            <Field label={`Host ${index + 1} domain`}>
              <Input value={host.domain} onChange={(e) => setHost(index, { domain: e.target.value })} placeholder="example.com" />
            </Field>
            <Field label="Values CSV">
              <Input value={host.values} onChange={(e) => setHost(index, { values: e.target.value })} placeholder="1.1.1.1, 1.0.0.1" />
            </Field>
            <Button variant="ghost" icon={<Trash2 size={16} />} onClick={() => onChange({ ...editor, dnsHosts: editor.dnsHosts.filter((_, current) => current !== index) })}>
              Remove
            </Button>
          </div>
        ))}
      </section>
    </>
  );
}

function TunEditor({ editor, onChange }: { editor: AdvancedConfigEditorState; onChange: (next: AdvancedConfigEditorState) => void }) {
  return (
    <section className="drawer-card configurator-section">
      <div className="section-editor-head">
        <div>
          <h3>TUN runtime</h3>
          <p>Common cross-platform TUN fields with advanced JSON fallback for anything platform-specific.</p>
        </div>
      </div>
      <div className="configurator-grid">
        <Field label="Name">
          <Input value={editor.tunName} onChange={(e) => onChange({ ...editor, tunName: e.target.value })} placeholder="blackwire-tun" />
        </Field>
        <Field label="Address">
          <Input value={editor.tunAddress} onChange={(e) => onChange({ ...editor, tunAddress: e.target.value })} placeholder="198.18.0.1" />
        </Field>
        <Field label="Netmask">
          <Input value={editor.tunNetmask} onChange={(e) => onChange({ ...editor, tunNetmask: e.target.value })} placeholder="255.255.0.0" />
        </Field>
        <Field label="MTU">
          <Input value={editor.tunMtu} onChange={(e) => onChange({ ...editor, tunMtu: e.target.value })} placeholder="1500" />
        </Field>
        <Field label="Bypass mark">
          <Input value={editor.tunBypassMark} onChange={(e) => onChange({ ...editor, tunBypassMark: e.target.value })} placeholder="4660" />
        </Field>
        <Field label="Redirect port">
          <Input value={editor.tunRedirectPort} onChange={(e) => onChange({ ...editor, tunRedirectPort: e.target.value })} placeholder="7890" />
        </Field>
        <Field label="DNS port">
          <Input value={editor.tunDnsPort} onChange={(e) => onChange({ ...editor, tunDnsPort: e.target.value })} placeholder="5300" />
        </Field>
      </div>
    </section>
  );
}

function ApiEditor({ editor, onChange }: { editor: AdvancedConfigEditorState; onChange: (next: AdvancedConfigEditorState) => void }) {
  return (
    <section className="drawer-card configurator-section">
      <h3>gRPC API listener</h3>
      <Field label="Listen">
        <Input value={editor.apiListen} onChange={(e) => onChange({ ...editor, apiListen: e.target.value })} placeholder="127.0.0.1:62789" />
      </Field>
    </section>
  );
}

function MetricsEditor({ editor, onChange }: { editor: AdvancedConfigEditorState; onChange: (next: AdvancedConfigEditorState) => void }) {
  return (
    <section className="drawer-card configurator-section">
      <h3>Prometheus metrics listener</h3>
      <Field label="Metrics address">
        <Input value={editor.metricsAddr} onChange={(e) => onChange({ ...editor, metricsAddr: e.target.value })} placeholder="127.0.0.1:9090" />
      </Field>
    </section>
  );
}

function ProfileEditor({ editor, onChange }: { editor: AdvancedConfigEditorState; onChange: (next: AdvancedConfigEditorState) => void }) {
  return (
    <section className="drawer-card configurator-section">
      <h3>Runtime profile</h3>
      <div className="configurator-grid">
        <Field label="Profile">
          <Select value={editor.profile} onChange={(e) => onChange({ ...editor, profile: e.target.value, profileCustom: e.target.value ? "" : editor.profileCustom })}>
            <option value="">Custom</option>
            {profileOptions.map((item) => (
              <option key={item} value={item}>
                {item}
              </option>
            ))}
          </Select>
        </Field>
        {!editor.profile ? (
          <Field label="Custom profile value">
            <Input value={editor.profileCustom} onChange={(e) => onChange({ ...editor, profileCustom: e.target.value })} placeholder="compat" />
          </Field>
        ) : null}
      </div>
    </section>
  );
}

function FastEditor({ editor, onChange }: { editor: AdvancedConfigEditorState; onChange: (next: AdvancedConfigEditorState) => void }) {
  return (
    <section className="drawer-card configurator-section">
      <div className="section-editor-head">
        <div>
          <h3>Fast tuning</h3>
          <p>Common production tuning knobs for the Blackwire fast profile.</p>
        </div>
      </div>
      <div className="configurator-grid">
        <Switch checked={editor.fastStrictProduction} onChange={(fastStrictProduction) => onChange({ ...editor, fastStrictProduction })} label="Strict production" />
        <Field label="Pool">
          <Select value={editor.fastPool} onChange={(e) => onChange({ ...editor, fastPool: e.target.value })}>
            {fastPoolOptions.map((item) => (
              <option key={item} value={item}>
                {item || "default"}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Splice">
          <Select value={editor.fastSplice} onChange={(e) => onChange({ ...editor, fastSplice: e.target.value })}>
            {fastSpliceOptions.map((item) => (
              <option key={item} value={item}>
                {item || "default"}
              </option>
            ))}
          </Select>
        </Field>
      </div>
    </section>
  );
}
