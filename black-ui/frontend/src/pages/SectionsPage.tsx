import { ArrowDown, ArrowUp, Database, GitBranch, Globe2, Plus, Route, Save, Trash2 } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import type { Outbound, RouteInput, RoutingDns } from "../lib/types";
import { Button } from "../components/atoms/Button";
import { Input, Select, Textarea } from "../components/atoms/Input";
import { Field } from "../components/molecules/Field";

const lines = (value: string) => value.split("\n").map((item) => item.trim()).filter(Boolean);
const text = (value: string[]) => value.join("\n");
const emptyRule = (outboundTag = ""): RouteInput => ({ ruleType: "field", port: null, outboundTag, domains: [], ips: [], inboundTags: [], protocols: [], users: [] });

function matcherCount(rule: RouteInput) {
  return rule.domains.length + rule.ips.length + rule.inboundTags.length + rule.protocols.length + rule.users.length + (rule.port ? 1 : 0);
}

function matcherSummary(rule: RouteInput) {
  const count = matcherCount(rule);
  if (count === 0) return "Matches all traffic";
  const groups = [rule.domains.length && "domain", rule.ips.length && "IP", rule.port && "port", rule.inboundTags.length && "inbound", rule.protocols.length && "protocol", rule.users.length && "user"].filter(Boolean);
  return `${count} ${count === 1 ? "matcher" : "matchers"} across ${groups.join(", ")}`;
}

export function SectionsPage({ value, outbounds, busy, onSave }: { value: RoutingDns; outbounds: Outbound[]; busy: boolean; onSave: (value: RoutingDns) => void }) {
  const [form, setForm] = useState(value);
  useEffect(() => setForm(value), [value]);
  const dirty = useMemo(() => JSON.stringify(form) !== JSON.stringify(value), [form, value]);
  const canAddRule = outbounds.length > 0;
  const updateRule = (index: number, patch: Partial<RouteInput>) => setForm((current) => ({ ...current, rules: current.rules.map((rule, at) => at === index ? { ...rule, ...patch } : rule) }));
  const moveRule = (index: number, direction: -1 | 1) => setForm((current) => {
    const next = [...current.rules];
    const target = index + direction;
    if (target < 0 || target >= next.length) return current;
    [next[index], next[target]] = [next[target], next[index]];
    return { ...current, rules: next };
  });

  return <div className="page routing-page">
    <header className="routing-page-head">
      <div>
        <span className="routing-eyebrow"><GitBranch size={14} /> Traffic policy</span>
        <h1>Routing &amp; DNS</h1>
        <p>Resolve destinations, then route traffic through the first matching rule.</p>
      </div>
      <div className="routing-save-cluster">
        <span className={`routing-change-state ${dirty ? "routing-change-state-dirty" : ""}`}>{dirty ? "Unsaved changes" : "Revision is current"}</span>
        <Button variant="primary" icon={<Save size={16} />} disabled={busy || !dirty} onClick={() => onSave(form)}>Save revision</Button>
      </div>
    </header>

    <div className="routing-workspace">
      <aside className="routing-dns-rail">
        <div className="routing-section-head">
          <span className="routing-section-icon"><Database size={18} /></span>
          <div><h2>DNS resolver</h2><p>Upstreams are tried in this order.</p></div>
        </div>
        <Field label="Resolution strategy" hint="AsIs preserves domains; IP modes resolve before routing.">
          <Select value={form.domainStrategy} onChange={(event) => setForm({ ...form, domainStrategy: event.target.value })}>
            <option value="AsIs">Keep domains (AsIs)</option>
            <option value="IPIfNonMatch">Resolve if no rule matches</option>
            <option value="IPOnDemand">Resolve before matching</option>
          </Select>
        </Field>
        <Field label="Upstream servers" hint="One address per line. Plain IP, DoH, and supported resolver URLs are accepted.">
          <Textarea className="routing-dns-textarea" rows={7} value={text(form.dnsServers)} placeholder={"1.1.1.1\nhttps://dns.example/dns-query"} onChange={(event) => setForm({ ...form, dnsServers: lines(event.target.value) })} />
        </Field>
        <div className="routing-rail-stat"><Globe2 size={16} /><span><strong>{form.dnsServers.length}</strong> configured {form.dnsServers.length === 1 ? "resolver" : "resolvers"}</span></div>
      </aside>

      <main className="routing-rules-surface">
        <div className="routing-rules-head">
          <div>
            <span className="routing-kicker">Ordered rules</span>
            <h2>Traffic routes <small>{form.rules.length}</small></h2>
            <p>Rules run top to bottom. The first match wins.</p>
          </div>
          <Button variant="secondary" icon={<Plus size={16} />} disabled={!canAddRule} onClick={() => setForm({ ...form, rules: [...form.rules, emptyRule(outbounds[0]?.tag)] })}>Add rule</Button>
        </div>

        <div className="routing-rule-list">
          {form.rules.map((rule, index) => <article className="routing-rule" key={index}>
            <header className="routing-rule-head">
              <span className="routing-rule-number">{String(index + 1).padStart(2, "0")}</span>
              <div className="routing-rule-title"><strong>{matcherSummary(rule)}</strong><span>Then send through <b>{rule.outboundTag || "an outbound"}</b></span></div>
              <div className="routing-rule-actions">
                <button type="button" className="routing-icon-action" aria-label={`Move rule ${index + 1} up`} disabled={index === 0} onClick={() => moveRule(index, -1)}><ArrowUp size={16} /></button>
                <button type="button" className="routing-icon-action" aria-label={`Move rule ${index + 1} down`} disabled={index === form.rules.length - 1} onClick={() => moveRule(index, 1)}><ArrowDown size={16} /></button>
                <button type="button" className="routing-icon-action routing-icon-action-danger" aria-label={`Remove rule ${index + 1}`} onClick={() => setForm({ ...form, rules: form.rules.filter((_, at) => at !== index) })}><Trash2 size={16} /></button>
              </div>
            </header>

            <div className="routing-rule-flow">
              <section className="routing-matchers">
                <div className="routing-subhead"><span>When</span><small>All populated groups must match</small></div>
                <div className="routing-primary-grid">
                  <Field label="Domains" hint="One domain expression per line."><Textarea rows={3} placeholder={"domain:example.com\ngeosite:private"} value={text(rule.domains)} onChange={(event) => updateRule(index, { domains: lines(event.target.value) })} /></Field>
                  <Field label="IP or CIDR" hint="One address or network per line."><Textarea rows={3} placeholder={"10.0.0.0/8\ngeoip:private"} value={text(rule.ips)} onChange={(event) => updateRule(index, { ips: lines(event.target.value) })} /></Field>
                </div>
                <div className="routing-secondary-grid">
                  <Field label="Ports"><Input value={rule.port ?? ""} placeholder="80,443,1000-2000" onChange={(event) => updateRule(index, { port: event.target.value || null })} /></Field>
                  <Field label="Protocols"><Textarea rows={2} placeholder={"http\ntls"} value={text(rule.protocols)} onChange={(event) => updateRule(index, { protocols: lines(event.target.value) })} /></Field>
                  <Field label="Inbound tags"><Textarea rows={2} placeholder="public-vless" value={text(rule.inboundTags)} onChange={(event) => updateRule(index, { inboundTags: lines(event.target.value) })} /></Field>
                  <Field label="Users"><Textarea rows={2} placeholder="user@example.com" value={text(rule.users)} onChange={(event) => updateRule(index, { users: lines(event.target.value) })} /></Field>
                </div>
              </section>
              <section className="routing-destination">
                <div className="routing-subhead"><span>Route to</span></div>
                <Field label="Outbound"><Select value={rule.outboundTag} onChange={(event) => updateRule(index, { outboundTag: event.target.value })}>{outbounds.map((outbound) => <option key={outbound.id} value={outbound.tag}>{outbound.tag}</option>)}</Select></Field>
                <div className="routing-flow-note"><Route size={16} /><span>Stops evaluation when this rule matches.</span></div>
              </section>
            </div>
          </article>)}

          {form.rules.length === 0 ? <div className="routing-empty">
            <span><Route size={22} /></span><h3>{canAddRule ? "No custom routes" : "No outbound available"}</h3>
            <p>{canAddRule ? "All traffic currently uses the default outbound." : "Connect MySQL and create an outbound before adding routing rules."}</p>
            <Button variant="secondary" icon={<Plus size={16} />} disabled={!canAddRule} onClick={() => setForm({ ...form, rules: [emptyRule(outbounds[0]?.tag)] })}>Create first rule</Button>
          </div> : null}
        </div>
      </main>
    </div>

    <div className="routing-mobile-save"><span>{dirty ? "Unsaved changes" : "Revision is current"}</span><Button variant="primary" icon={<Save size={16} />} disabled={busy || !dirty} onClick={() => onSave(form)}>Save revision</Button></div>
  </div>;
}
