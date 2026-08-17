import { Save, Trash2, X } from "lucide-react";
import { useEffect, useState } from "react";
import type { CapabilityMap, Outbound, OutboundInput } from "../../lib/types";
import { Button } from "../atoms/Button";
import { IconButton } from "../atoms/IconButton";
import { Input, Select } from "../atoms/Input";
import { Switch } from "../atoms/Switch";
import { Field } from "../molecules/Field";

const emptyOutbound: OutboundInput = { tag: "", protocol: "freedom", enabled: true, address: null, port: null, transport: "tcp", security: "none" };

export function OutboundDrawer({ editing, capabilities, busy, onClose, onCreate, onUpdate, onDelete }: {
  editing: Outbound | null; capabilities: CapabilityMap | null; busy: boolean; onClose: () => void;
  onCreate: (input: OutboundInput) => void; onUpdate: (id: number, input: OutboundInput) => void; onDelete: (id: number) => void;
}) {
  const [form, setForm] = useState<OutboundInput>(emptyOutbound);
  useEffect(() => { setForm(editing ? { tag: editing.tag, protocol: editing.protocol, enabled: editing.enabled, address: editing.address, port: editing.port, transport: editing.transport, security: editing.security } : emptyOutbound); }, [editing]);
  const valid = form.tag.trim() !== "" && (form.protocol === "freedom" || Boolean(form.address?.trim()));
  const save = () => { if (!valid || busy) return; editing ? onUpdate(editing.id, form) : onCreate(form); onClose(); };

  return <aside className="drawer">
    <div className="drawer-head"><div><h2>{editing ? editing.tag : "New outbound"}</h2><p>Typed destination settings stored in a relational revision.</p></div><IconButton label="Close" onClick={onClose}><X size={18} /></IconButton></div>
    <div className="drawer-body">
      <section className="drawer-card"><div className="summary-head"><strong>{form.tag || "Untitled outbound"}</strong><Switch checked={form.enabled} onChange={(enabled) => setForm({ ...form, enabled })} label={form.enabled ? "Enabled" : "Disabled"} /></div></section>
      <section className="drawer-card configurator-grid">
        <Field label="Tag"><Input value={form.tag} onChange={(event) => setForm({ ...form, tag: event.target.value })} /></Field>
        <Field label="Protocol"><Select value={form.protocol} onChange={(event) => setForm({ ...form, protocol: event.target.value })}>{(capabilities?.protocols ?? []).filter((item) => ["freedom", "vless", "vmess", "trojan", "shadowsocks", "hysteria2", "tuic"].includes(item.key)).map((item) => <option key={item.key} value={item.key}>{item.label}</option>)}</Select></Field>
        {form.protocol !== "freedom" ? <><Field label="Server address"><Input value={form.address ?? ""} onChange={(event) => setForm({ ...form, address: event.target.value })} /></Field><Field label="Server port"><Input type="number" min={1} max={65535} value={form.port ?? ""} onChange={(event) => setForm({ ...form, port: event.target.value ? Number(event.target.value) : null })} /></Field></> : null}
        <Field label="Transport"><Select value={form.transport} onChange={(event) => setForm({ ...form, transport: event.target.value })}>{(capabilities?.transports ?? []).filter((item) => ["tcp", "ws", "grpc", "httpupgrade", "splithttp", "quic", "kcp"].includes(item.key)).map((item) => <option key={item.key} value={item.key}>{item.label}</option>)}</Select></Field>
        <Field label="Security"><Select value={form.security} onChange={(event) => setForm({ ...form, security: event.target.value })}>{(capabilities?.security ?? []).map((item) => <option key={item.key} value={item.key}>{item.label}</option>)}</Select></Field>
      </section>
      <p className="field-hint">Exclusive bindings stay pending until a maintenance activation is confirmed.</p>
    </div>
    <div className="drawer-actions">{editing ? <Button variant="danger" icon={<Trash2 size={16} />} disabled={busy} onClick={() => onDelete(editing.id)}>Delete</Button> : <span />}<Button variant="primary" icon={<Save size={16} />} disabled={!valid || busy} loading={busy} onClick={save}>Save revision</Button></div>
  </aside>;
}
