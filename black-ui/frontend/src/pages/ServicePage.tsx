import { RotateCcw, Wrench } from "lucide-react";
import type { RevisionSummary, Status } from "../lib/types";
import { Badge } from "../components/atoms/Badge";
import { Button } from "../components/atoms/Button";

export function ServicePage({ status, revisions, busy, onRollback, onActivateMaintenance }: {
  status: Status | null; revisions: RevisionSummary[]; busy: boolean;
  onRollback: (revision: number) => void; onActivateMaintenance: (revision: number) => void;
}) {
  return <div className="page">
    <div className="page-title"><h1>Runtime</h1><p>Revision activation, maintenance review, rollback, and reconciliation health.</p></div>
    <section className="work-panel split-panel">
      <div><h2>Reconciliation</h2><div className="mini-list">
        <div><span>MySQL</span><Badge tone={status?.databaseConnected ? "green" : "red"}>{status?.databaseConnected ? "connected" : "unavailable"}</Badge></div>
        <div><span>Desired revision</span><strong>{status?.desiredRevision ?? "—"}</strong></div>
        <div><span>Active revision</span><strong>{status?.activeRevision ?? "—"}</strong></div>
        <div><span>Activation</span><Badge tone={status?.activationState === "failed" ? "red" : status?.activationState === "pendingMaintenance" ? "amber" : "green"}>{status?.activationState ?? "unknown"}</Badge></div>
      </div>{status?.lastActivationError ? <p className="error-line">{status.lastActivationError}</p> : null}
      {status?.pendingMaintenanceRevision ? <Button variant="primary" icon={<Wrench size={16} />} disabled={busy} onClick={() => onActivateMaintenance(status.pendingMaintenanceRevision!)}>Confirm revision {status.pendingMaintenanceRevision}</Button> : null}</div>
      <div><h2>Activation policy</h2><p className="field-hint">Hot state swaps activate automatically. Reusable listeners are prepared and handed over. Exclusive resources wait for explicit maintenance confirmation.</p></div>
    </section>
    <section className="work-panel"><div className="panel-toolbar"><div><h2>Revision history</h2><p>Latest 20 immutable revisions.</p></div></div><div className="table-wrap"><table><thead><tr><th>Revision</th><th>Summary</th><th>Actor</th><th>Class</th><th>Created</th><th /></tr></thead><tbody>{revisions.map((revision) => <tr key={revision.revision}><td><strong>#{revision.revision}</strong>{revision.revision === status?.activeRevision ? <small>active</small> : null}</td><td>{revision.summary}</td><td>{revision.actor}</td><td><Badge tone={revision.activationClass === "maintenanceRequired" ? "amber" : "cyan"}>{revision.activationClass}</Badge></td><td>{new Date(revision.createdAt).toLocaleString()}</td><td><Button variant="secondary" icon={<RotateCcw size={15} />} disabled={busy || revision.revision === status?.desiredRevision} onClick={() => onRollback(revision.revision)}>Rollback</Button></td></tr>)}</tbody></table></div></section>
  </div>;
}
