import { CirclePlay, RotateCcw, Square } from "lucide-react";
import type { RevisionSummary, ServiceStatus, Status } from "../lib/types";
import { Badge } from "../components/atoms/Badge";
import { Button } from "../components/atoms/Button";

export function ServicePage({ status, service, revisions, busy, onStart, onStop, onRollback }: {
  status: Status | null; service: ServiceStatus | null; revisions: RevisionSummary[]; busy: boolean;
  onStart: () => void; onStop: () => void; onRollback: (revision: number) => void;
}) {
  return <div className="page">
    <div className="page-title"><h1>Runtime</h1><p>Automatic live reload, rollback, and reconciliation health.</p></div>
    <section className="work-panel split-panel">
      <div><h2>Reconciliation</h2><div className="mini-list">
        <div><span>MySQL</span><Badge tone={status?.databaseConnected ? "green" : "red"}>{status?.databaseConnected ? "connected" : "unavailable"}</Badge></div>
        <div><span>Desired revision</span><strong>{status?.desiredRevision ?? "—"}</strong></div>
        <div><span>Active revision</span><strong>{status?.activeRevision ?? "—"}</strong></div>
        <div><span>Reload</span><Badge tone={status?.activationState === "failed" ? "red" : "green"}>{status?.activationState ?? "unknown"}</Badge></div>
      </div>{status?.lastActivationError ? <p className="error-line">{status.lastActivationError}</p> : null}</div>
      <div><h2>Reload policy</h2><p className="field-hint">Every valid revision applies automatically. Lightweight state changes swap atomically; structural changes use a prepared in-process handover while existing connections drain.</p></div>
    </section>
    <section className="work-panel runtime-service-panel"><div className="panel-toolbar"><div><h2>Blackwire service</h2><p>Host service state and recent runtime output.</p></div><Badge tone={service?.activeState === "active" ? "green" : service?.systemdAvailable ? "amber" : "gray"}>{service?.activeState ?? "unknown"}</Badge></div><div className="runtime-service-actions"><Button variant="secondary" icon={<CirclePlay size={15} />} disabled={busy || !service?.systemdAvailable || service.activeState === "active"} onClick={onStart}>Start</Button><Button variant="secondary" icon={<Square size={14} />} disabled={busy || !service?.systemdAvailable || service.activeState !== "active"} onClick={onStop}>Stop</Button><span>{service?.subState ?? "Service manager unavailable"}</span></div><pre className="runtime-log-view">{service?.logs.length ? service.logs.join("\n") : "No journal entries available on this host."}</pre></section>
    <section className="work-panel"><div className="panel-toolbar"><div><h2>Revision history</h2><p>Latest 20 immutable revisions.</p></div></div><div className="table-wrap"><table><thead><tr><th>Revision</th><th>Summary</th><th>Actor</th><th>Apply path</th><th>Created</th><th /></tr></thead><tbody>{revisions.map((revision) => <tr key={revision.revision}><td><strong>#{revision.revision}</strong>{revision.revision === status?.activeRevision ? <small>active</small> : null}</td><td>{revision.summary}</td><td>{revision.actor}</td><td><Badge tone="cyan">{revision.activationClass}</Badge></td><td>{new Date(revision.createdAt).toLocaleString()}</td><td><Button variant="secondary" icon={<RotateCcw size={15} />} disabled={busy || revision.revision === status?.desiredRevision} onClick={() => onRollback(revision.revision)}>Rollback</Button></td></tr>)}</tbody></table></div></section>
  </div>;
}
