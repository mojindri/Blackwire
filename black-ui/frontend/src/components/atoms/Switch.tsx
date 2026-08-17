import { HelpTooltip, type HelpContent } from "./HelpTooltip";

interface SwitchProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
  help?: HelpContent;
}

export function Switch({ checked, onChange, label, help }: SwitchProps) {
  if (help) return <div className="switch-with-help"><span className="switch-help-label"><button className="switch-label-button" type="button" onClick={() => onChange(!checked)}>{label}</button><HelpTooltip label={label} content={help} /></span><button className={`switch-control ${checked ? "switch-on" : ""}`} role="switch" aria-label={label} aria-checked={checked} onClick={() => onChange(!checked)} type="button"><span className="switch-track"><span className="switch-thumb" /></span></button></div>;
  return (
    <button
      className={`switch ${checked ? "switch-on" : ""}`}
      role="switch"
      aria-checked={checked}
      onClick={() => onChange(!checked)}
      type="button"
    >
      <span>{label}</span>
      <span className="switch-track">
        <span className="switch-thumb" />
      </span>
    </button>
  );
}
