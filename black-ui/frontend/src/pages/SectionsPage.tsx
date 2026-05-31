import { useEffect, useState } from "react";
import { Save, Wand2 } from "lucide-react";
import type { CapabilityMap, ConfigSection, Outbound } from "../lib/types";
import { Badge } from "../components/atoms/Badge";
import { Button } from "../components/atoms/Button";
import { Textarea } from "../components/atoms/Input";
import { Switch } from "../components/atoms/Switch";
import { Field } from "../components/molecules/Field";

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
  const [selected, setSelected] = useState<ConfigSection | null>(null);
  const [enabled, setEnabled] = useState(false);
  const [value, setValue] = useState("");
  const notes = new Map((capabilities?.config ?? []).map((item) => [item.key, item]));

  useEffect(() => {
    if (!selected && sections.length > 0) setSelected(sections[0]);
  }, [sections, selected]);

  useEffect(() => {
    if (!selected) return;
    setEnabled(selected.enabled);
    setValue(selected.value);
  }, [selected]);

  const adaptiveTemplateAvailable = selected?.name === "routing" && outbounds.filter((outbound) => outbound.enabled).length >= 2;
  const insertAdaptiveTemplate = () => {
    const enabledOutbounds = outbounds.filter((outbound) => outbound.enabled).slice(0, 2);
    if (enabledOutbounds.length < 2) return;
    const [primary, backup] = enabledOutbounds;
    setEnabled(true);
    setValue(
      JSON.stringify(
        {
          balancers: [
            {
              tag: "auto-proxy",
              selector: [primary.tag, backup.tag],
              strategy: "adaptive",
              profiles: [
                { name: "stable", outboundTag: primary.tag },
                { name: "backup", outboundTag: backup.tag }
              ],
              adaptive: {
                failureThreshold: 2,
                cooldownSecs: 30,
                ewmaAlpha: 0.2,
                switchMargin: 0.15
              },
              health_check: {
                url: "http://www.gstatic.com/generate_204",
                interval_secs: 30,
                timeout_secs: 5,
                max_failures: 2
              }
            }
          ],
          rules: [{ outboundTag: "auto-proxy" }]
        },
        null,
        2
      )
    );
  };

  return (
    <div className="page">
      <div className="page-title">
        <h1>Config Sections</h1>
        <p>Raw validated Blackwire JSON for routing, DNS, TUN, metrics, profile, and API coverage.</p>
      </div>
      <div className="two-column">
        <section className="work-panel">
          <h2>Sections</h2>
          <div className="stack-list">
            {sections.map((section) => {
              const cap = notes.get(section.name);
              return (
                <button className="stack-row" key={section.name} onClick={() => setSelected(section)} type="button">
                  <span>
                    <strong>{section.name}</strong>
                    <small>{cap?.notes ?? "Blackwire native config section"}</small>
                  </span>
                  <Badge tone={section.enabled ? "green" : "gray"}>{cap?.status ?? "supported"}</Badge>
                </button>
              );
            })}
          </div>
        </section>
        <section className="work-panel editor-panel">
          <div className="section-editor-head">
            <h2>{selected ? selected.name : "Select section"}</h2>
            {selected?.name === "routing" ? (
              <Button
                variant="secondary"
                icon={<Wand2 size={16} />}
                onClick={insertAdaptiveTemplate}
                disabled={busy || !adaptiveTemplateAvailable}
              >
                Adaptive Template
              </Button>
            ) : null}
          </div>
          {selected ? (
            <>
              <Switch checked={enabled} onChange={setEnabled} label="Include section in generated config" />
              <Field label="JSON value">
                <Textarea rows={18} value={value} onChange={(e) => setValue(e.target.value)} />
              </Field>
              <Button variant="primary" icon={<Save size={16} />} onClick={() => onSave(selected.name, enabled, value)} disabled={busy}>
                Save Section
              </Button>
            </>
          ) : null}
        </section>
      </div>
    </div>
  );
}
