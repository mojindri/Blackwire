import { Save, X } from "lucide-react";
import { useEffect, useState } from "react";
import type { Inbound, ManagedUser, Settings, UserInput } from "../../lib/types";
import { Button } from "../atoms/Button";
import { IconButton } from "../atoms/IconButton";
import { Input, Select, Textarea } from "../atoms/Input";
import { Switch } from "../atoms/Switch";
import { Field } from "../molecules/Field";

export function UserDrawer({ open, user, inbounds, busy, onClose, onSubmit, onUuid }: {
  open: boolean; user: ManagedUser | null; inbounds: Inbound[]; settings: Settings | null; busy: boolean;
  onClose: () => void; onSubmit: (id: number | null, input: UserInput) => void; onUuid: () => Promise<string>;
  onRotateUuid: (user: ManagedUser) => void; onRotateToken: (user: ManagedUser) => void; onReset: (user: ManagedUser) => void;
}) {
  const [form, setForm] = useState<UserInput>({ inboundId: 0, email: "", uuid: "", flow: "", note: "", enabled: true, trafficLimitBytes: null, expiryAt: null, credentialKind: "uuid" });
  useEffect(() => {
    setForm(user ? { inboundId: user.inboundId, email: user.email, uuid: user.uuid, flow: user.flow, note: user.note, enabled: user.enabled, trafficLimitBytes: user.trafficLimitBytes, expiryAt: user.expiryAt, credentialKind: user.credentialKind, method: user.method ?? undefined, subscriptionToken: user.subscriptionToken } : { inboundId: inbounds[0]?.id ?? 0, email: "", uuid: "", flow: "", note: "", enabled: true, trafficLimitBytes: null, expiryAt: null, credentialKind: "uuid" });
  }, [user, inbounds, open]);
  if (!open) return null;
  const valid = form.inboundId > 0 && form.email.trim() !== "" && form.uuid.trim() !== "";
  return <aside className="drawer">
    <div className="drawer-head"><div><h2>{user ? user.email : "New user"}</h2><p>One typed credential attached to one inbound.</p></div><IconButton label="Close" onClick={onClose}><X size={18} /></IconButton></div>
    <div className="drawer-body"><section className="drawer-card configurator-grid">
      <Field label="Inbound"><Select value={form.inboundId} onChange={(event) => setForm({ ...form, inboundId: Number(event.target.value) })}>{inbounds.map((inbound) => <option key={inbound.id} value={inbound.id}>{inbound.tag}</option>)}</Select></Field>
      <Field label="Email"><Input type="email" value={form.email} onChange={(event) => setForm({ ...form, email: event.target.value })} /></Field>
      <Field label="UUID"><div className="inline-actions"><Input value={form.uuid} onChange={(event) => setForm({ ...form, uuid: event.target.value })} /><Button variant="secondary" onClick={async () => setForm({ ...form, uuid: await onUuid() })}>Generate</Button></div></Field>
      <Field label="Flow"><Input value={form.flow ?? ""} onChange={(event) => setForm({ ...form, flow: event.target.value })} /></Field>
      <Field label="Traffic limit (bytes)"><Input type="number" min={0} value={form.trafficLimitBytes ?? ""} onChange={(event) => setForm({ ...form, trafficLimitBytes: event.target.value ? Number(event.target.value) : null })} /></Field>
      <Field label="Expiry"><Input type="datetime-local" value={form.expiryAt?.slice(0, 16) ?? ""} onChange={(event) => setForm({ ...form, expiryAt: event.target.value ? new Date(event.target.value).toISOString() : null })} /></Field>
      <Field label="Note"><Textarea rows={3} value={form.note ?? ""} onChange={(event) => setForm({ ...form, note: event.target.value })} /></Field>
      <Switch checked={form.enabled} onChange={(enabled) => setForm({ ...form, enabled })} label={form.enabled ? "Enabled" : "Disabled"} />
    </section></div>
    <div className="drawer-actions"><span /><Button variant="primary" icon={<Save size={16} />} disabled={!valid || busy} loading={busy} onClick={() => onSubmit(user?.id ?? null, form)}>Save revision</Button></div>
  </aside>;
}
