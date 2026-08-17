import { Save, Trash2, X } from "lucide-react";
import { useEffect, useState } from "react";
import type { CapabilityMap, Inbound, InboundInput } from "../../lib/types";
import { Button } from "../atoms/Button";
import { IconButton } from "../atoms/IconButton";
import { Input, Select } from "../atoms/Input";
import { Switch } from "../atoms/Switch";
import { Field } from "../molecules/Field";

const emptyInbound: InboundInput = { tag: "", listen: "0.0.0.0", port: 443, protocol: "vless", enabled: true, transport: "tcp", security: "none" };

export function InboundDrawer({ editing, inboundsCount, capabilities, busy, onClose, onCreate, onUpdate, onDelete }: {
  editing: Inbound | null; inboundsCount: number; capabilities: CapabilityMap | null; busy: boolean;
  onClose: () => void; onCreate: (input: InboundInput) => void; onUpdate: (id: number, input: InboundInput) => void; onDelete: (id: number) => void;
}) {
  const [form, setForm] = useState<InboundInput>(emptyInbound);
  useEffect(() => { setForm(editing ? { tag: editing.tag, listen: editing.listen, port: editing.port, protocol: editing.protocol, enabled: editing.enabled, transport: editing.transport, security: editing.security } : emptyInbound); }, [editing]);
  const valid = form.tag.trim() !== "" && form.listen.trim() !== "" && form.port > 0 && form.port <= 65535;
  const save = () => { if (!valid || busy) return; editing ? onUpdate(editing.id, form) : onCreate(form); onClose(); };

  return <aside className="drawer">
    <div className="drawer-head"><div><h2>{editing ? editing.tag : "New inbound"}</h2><p>Typed listener settings. Saving creates an immutable revision.</p></div><IconButton label="Close" onClick={onClose}><X size={18} /></IconButton></div>
    <div className="drawer-body">
      <section className="drawer-card"><div className="summary-head"><strong>{form.tag || "Untitled inbound"}</strong><Switch checked={form.enabled} onChange={(enabled) => setForm({ ...form, enabled })} label={form.enabled ? "Enabled" : "Disabled"} /></div></section>
      <section className="drawer-card configurator-grid">
        <Field label="Tag"><Input value={form.tag} onChange={(event) => setForm({ ...form, tag: event.target.value })} /></Field>
        <Field label="Listen address"><Input value={form.listen} onChange={(event) => setForm({ ...form, listen: event.target.value })} /></Field>
        <Field label="Port"><Input type="number" min={1} max={65535} value={form.port} onChange={(event) => setForm({ ...form, port: Number(event.target.value) })} /></Field>
        <Field label="Protocol"><Select value={form.protocol} onChange={(event) => setForm({ ...form, protocol: event.target.value })}>{(capabilities?.protocols ?? []).filter((item) => ["vless", "vmess", "trojan", "shadowsocks", "hysteria2", "tuic", "socks", "http"].includes(item.key)).map((item) => <option key={item.key} value={item.key}>{item.label}</option>)}</Select></Field>
        <Field label="Transport"><Select value={form.transport} onChange={(event) => setForm({ ...form, transport: event.target.value })}>{(capabilities?.transports ?? []).filter((item) => ["tcp", "ws", "grpc", "httpupgrade", "splithttp", "quic", "kcp"].includes(item.key)).map((item) => <option key={item.key} value={item.key}>{item.label}</option>)}</Select></Field>
        <Field label="Security"><Select value={form.security} onChange={(event) => setForm({ ...form, security: event.target.value })}>{(capabilities?.security ?? []).map((item) => <option key={item.key} value={item.key}>{item.label}</option>)}</Select></Field>
      </section>
      <p className="field-hint">Protocol-specific options are represented by typed relational fields. The API never accepts a complete raw configuration document.</p>
    </div>
    <div className="drawer-actions">{editing ? <Button variant="danger" icon={<Trash2 size={16} />} disabled={busy || inboundsCount <= 1} onClick={() => onDelete(editing.id)}>Delete</Button> : <span />}<Button variant="primary" icon={<Save size={16} />} disabled={!valid || busy} loading={busy} onClick={save}>Save revision</Button></div>
  </aside>;
}
