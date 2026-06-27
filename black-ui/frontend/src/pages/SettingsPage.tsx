import { useEffect, useRef, useState } from "react";
import { Save } from "lucide-react";
import type { Settings } from "../lib/types";
import { Button } from "../components/atoms/Button";
import { Input } from "../components/atoms/Input";
import { Switch } from "../components/atoms/Switch";
import { Field } from "../components/molecules/Field";

export function SettingsPage({
  settings,
  busy,
  onSave
}: {
  settings: Settings | null;
  busy: boolean;
  onSave: (settings: Settings) => void;
}) {
  const [form, setForm] = useState<Settings | null>(settings);
  const formRef = useRef(form);
  useEffect(() => {
    formRef.current = settings;
    setForm(settings);
  }, [settings]);
  if (!form) return <div className="page">Loading settings...</div>;

  const updateForm = (patch: Partial<Settings>) => {
    const current = formRef.current;
    if (!current) return;
    const next = { ...current, ...patch };
    formRef.current = next;
    setForm(next);
  };

  const save = () => {
    const latest = formRef.current;
    if (latest) onSave(latest);
  };

  return (
    <div className="page">
      <div className="page-title">
        <h1>Settings</h1>
        <p>Local panel paths, runtime gRPC, and subscription host.</p>
      </div>
      <section className="work-panel settings-panel">
        <Field label="Config path">
          <Input value={form.configPath} onChange={(e) => updateForm({ configPath: e.target.value })} />
        </Field>
        <Switch checked={form.grpcEnabled} onChange={(grpcEnabled) => updateForm({ grpcEnabled })} label="Use live gRPC apply and traffic" />
        <Field label="gRPC address">
          <Input value={form.grpcAddress} onChange={(e) => updateForm({ grpcAddress: e.target.value })} />
        </Field>
        <Switch
          checked={form.firewallAutoOpen}
          onChange={(firewallAutoOpen) => updateForm({ firewallAutoOpen })}
          label="Auto-open UFW ports for public enabled inbounds"
        />
        <Switch
          checked={form.adaptiveRoutingEnabled}
          onChange={(adaptiveRoutingEnabled) => updateForm({ adaptiveRoutingEnabled })}
          label="Auto adaptive routing for enabled outbounds"
        />
        <Field label="Public base URL">
          <Input value={form.publicBaseUrl} onChange={(e) => updateForm({ publicBaseUrl: e.target.value })} />
        </Field>
        <Field label="Subscription host">
          <Input value={form.subscriptionHost} onChange={(e) => updateForm({ subscriptionHost: e.target.value })} />
        </Field>
        <Field label="Enforcement interval seconds">
          <Input
            type="number"
            min={5}
            value={form.enforcementIntervalSeconds}
            onChange={(e) => updateForm({ enforcementIntervalSeconds: Number(e.target.value) })}
          />
        </Field>
        <Button variant="primary" icon={<Save size={16} />} onClick={save} disabled={busy}>
          Save Settings
        </Button>
      </section>
    </div>
  );
}
