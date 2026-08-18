import { cloneElement, isValidElement, useId } from "react";
import type { ReactElement, ReactNode } from "react";
import { HelpTooltip, type HelpContent } from "../atoms/HelpTooltip";

export function Field({
  label,
  hint,
  help,
  children
}: {
  label: string;
  hint?: string;
  help?: HelpContent;
  children: ReactNode;
}) {
  const generatedId = useId();
  let controlId: string | undefined;
  let content = children;

  if (isValidElement(children)) {
    const child = children as ReactElement<{ id?: string }>;
    controlId = child.props.id ?? generatedId;
    content = cloneElement(child, { id: controlId });
  }

  return (
    <div className="field">
      <span className="field-label-row"><label className="field-label" htmlFor={controlId}>{label}</label>{help ? <HelpTooltip label={label} content={help} /> : null}</span>
      {content}
      {hint ? <span className="field-hint">{hint}</span> : null}
    </div>
  );
}
