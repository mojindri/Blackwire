import { Database, Plus, Route, Save, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import type { Outbound, RouteInput, RoutingDns } from "../lib/types";
import { Button } from "../components/atoms/Button";
import { Input, Select, Textarea } from "../components/atoms/Input";
import { Field } from "../components/molecules/Field";

const lines = (value: string) => value.split("\n").map((item) => item.trim()).filter(Boolean);
const text = (value: string[]) => value.join("\n");
const emptyRule = (outboundTag = ""): RouteInput => ({ ruleType: "field", port: null, outboundTag, domains: [], ips: [], inboundTags: [], protocols: [], users: [] });

export function SectionsPage({ value, outbounds, busy, onSave }: { value: RoutingDns; outbounds: Outbound[]; busy: boolean; onSave: (value: RoutingDns) => void }) {
  const [form, setForm] = useState(value);
  useEffect(() => setForm(value), [value]);
  const updateRule = (index: number, patch: Partial<RouteInput>) => setForm((current) => ({ ...current, rules: current.rules.map((rule, at) => at === index ? { ...rule, ...patch } : rule) }));
  return <div className="page">
    <div className="page-title"><h1>Routing &amp; DNS</h1><p>Ordered, typed resolver and route fields. Saving creates one immutable revision.</p></div>
    <div className="two-column">
      <section className="work-panel">
        <div className="panel-toolbar"><div><h2><Database size={18} /> DNS</h2><p>One upstream per line, in lookup order.</p></div></div>
        <Field label="Domain strategy"><Select value={form.domainStrategy} onChange={(event) => setForm({ ...form, domainStrategy: event.target.value })}><option>AsIs</option><option>IPIfNonMatch</option><option>IPOnDemand</option></Select></Field>
        <Field label="DNS servers"><Textarea rows={8} value={text(form.dnsServers)} placeholder={"1.1.1.1\nhttps://dns.example/dns-query"} onChange={(event) => setForm({ ...form, dnsServers: lines(event.target.value) })} /></Field>
      </section>
      <section className="work-panel">
        <div className="panel-toolbar"><div><h2><Route size={18} /> Routing</h2><p>Rules are evaluated from top to bottom.</p></div><Button variant="secondary" icon={<Plus size={16} />} onClick={() => setForm({ ...form, rules: [...form.rules, emptyRule(outbounds[0]?.tag)] })}>Add rule</Button></div>
        <div className="mini-list">
          {form.rules.map((rule, index) => <section className="drawer-card" key={index}>
            <div className="summary-head"><strong>Rule {index + 1}</strong><Button variant="danger" icon={<Trash2 size={15} />} onClick={() => setForm({ ...form, rules: form.rules.filter((_, at) => at !== index) })}>Remove</Button></div>
            <div className="configurator-grid">
              <Field label="Outbound"><Select value={rule.outboundTag} onChange={(event) => updateRule(index, { outboundTag: event.target.value })}>{outbounds.map((outbound) => <option key={outbound.id} value={outbound.tag}>{outbound.tag}</option>)}</Select></Field>
              <Field label="Port expression"><Input value={rule.port ?? ""} placeholder="80,443,1000-2000" onChange={(event) => updateRule(index, { port: event.target.value || null })} /></Field>
              <Field label="Domains"><Textarea rows={3} value={text(rule.domains)} onChange={(event) => updateRule(index, { domains: lines(event.target.value) })} /></Field>
              <Field label="IP/CIDR"><Textarea rows={3} value={text(rule.ips)} onChange={(event) => updateRule(index, { ips: lines(event.target.value) })} /></Field>
              <Field label="Inbound tags"><Textarea rows={2} value={text(rule.inboundTags)} onChange={(event) => updateRule(index, { inboundTags: lines(event.target.value) })} /></Field>
              <Field label="Protocols"><Textarea rows={2} value={text(rule.protocols)} onChange={(event) => updateRule(index, { protocols: lines(event.target.value) })} /></Field>
              <Field label="Users"><Textarea rows={2} value={text(rule.users)} onChange={(event) => updateRule(index, { users: lines(event.target.value) })} /></Field>
            </div>
          </section>)}
          {form.rules.length === 0 ? <p>No custom routes. Traffic uses the default outbound.</p> : null}
        </div>
      </section>
    </div>
    <div className="drawer-actions"><span /><Button variant="primary" icon={<Save size={16} />} disabled={busy} onClick={() => onSave(form)}>Save revision</Button></div>
  </div>;
}
