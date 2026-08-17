import { Database, Route } from "lucide-react";
import type { Status } from "../lib/types";

export function SectionsPage({ status }: { status: Status | null }) {
  return <div className="page">
    <div className="page-title"><h1>Routing &amp; DNS</h1><p>Typed routing rules, ordered selectors, balancers, and DNS upstreams are stored with configuration revisions.</p></div>
    <div className="two-column">
      <section className="work-panel"><div className="panel-toolbar"><div><h2><Route size={18} /> Routing</h2><p>Rules are evaluated in database order.</p></div></div><div className="empty">No custom routing rules in desired revision {status?.desiredRevision ?? "—"}.</div></section>
      <section className="work-panel"><div className="panel-toolbar"><div><h2><Database size={18} /> DNS</h2><p>System resolver is the current relational baseline.</p></div></div><div className="empty">System DNS is active. Add upstreams through typed resolver controls.</div></section>
    </div>
  </div>;
}
