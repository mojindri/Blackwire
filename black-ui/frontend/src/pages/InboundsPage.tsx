import { Plus } from "lucide-react";
import { useMemo, useState } from "react";
import { Button } from "../components/atoms/Button";
import { Badge } from "../components/atoms/Badge";
import { InboundDrawer } from "../components/organisms/InboundDrawer";
import { SearchBar } from "../components/molecules/SearchBar";
import { inboundSummary } from "../lib/inboundConfigurator";
import type { CapabilityMap, Inbound, InboundInput } from "../lib/types";

export function InboundsPage({
  inbounds,
  capabilities,
  busy,
  onCreate,
  onUpdate,
  onDelete
}: {
  inbounds: Inbound[];
  capabilities: CapabilityMap | null;
  busy: boolean;
  onCreate: (input: InboundInput) => void;
  onUpdate: (id: number, input: InboundInput) => void;
  onDelete: (id: number) => void;
}) {
  const [query, setQuery] = useState("");
  const [editing, setEditing] = useState<Inbound | null>(null);
  const [drawerOpen, setDrawerOpen] = useState(false);

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return inbounds;
    return inbounds.filter((inbound) => {
      const summary = inboundSummary(inbound);
      return [inbound.tag, inbound.listen, inbound.protocol, inbound.transport, summary.network, summary.security, summary.detail]
        .join(" ")
        .toLowerCase()
        .includes(needle);
    });
  }, [inbounds, query]);

  const openEditor = (inbound: Inbound | null) => {
    setEditing(inbound);
    setDrawerOpen(true);
  };

  return (
    <div className="page">
      <div className="page-title">
        <h1>Inbounds</h1>
        <p>Structured inbound definitions with guided protocol, transport, security, and advanced fallback only where it actually helps.</p>
      </div>

      <section className="work-panel">
        <div className="panel-toolbar">
          <SearchBar value={query} onChange={setQuery} />
          <Button variant="primary" icon={<Plus size={16} />} onClick={() => openEditor(null)} disabled={busy}>
            New Inbound
          </Button>
        </div>
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Tag</th>
                <th>Listen</th>
                <th>Protocol</th>
                <th>Transport</th>
                <th>Security</th>
                <th>Status</th>
              </tr>
            </thead>
            <tbody>
              {filtered.map((inbound) => {
                const summary = inboundSummary(inbound);
                return (
                  <tr key={inbound.id}>
                    <td>
                      <button className="link-cell" onClick={() => openEditor(inbound)} type="button">
                        {inbound.tag}
                      </button>
                      <small>{summary.detail || `${summary.network} transport`}</small>
                    </td>
                    <td>{inbound.listen}:{inbound.port}</td>
                    <td>{inbound.protocol}</td>
                    <td>{summary.network}</td>
                    <td>
                      <div className="table-chips">
                        <Badge tone={summary.security === "none" ? "gray" : "cyan"}>{summary.security}</Badge>
                      </div>
                    </td>
                    <td>
                      <Badge tone={inbound.enabled ? "green" : "gray"}>{inbound.enabled ? "enabled" : "disabled"}</Badge>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          {filtered.length === 0 ? <div className="empty">No inbounds match the current view.</div> : null}
        </div>
      </section>

      {drawerOpen ? (
        <InboundDrawer
          editing={editing}
          inboundsCount={inbounds.length}
          capabilities={capabilities}
          busy={busy}
          onClose={() => setDrawerOpen(false)}
          onCreate={onCreate}
          onUpdate={onUpdate}
          onDelete={onDelete}
        />
      ) : null}
    </div>
  );
}
