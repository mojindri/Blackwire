import { Plus } from "lucide-react";
import { useMemo, useState } from "react";
import { Badge } from "../components/atoms/Badge";
import { Button } from "../components/atoms/Button";
import { SearchBar } from "../components/molecules/SearchBar";
import { OutboundDrawer } from "../components/organisms/OutboundDrawer";
import type { CapabilityMap, Outbound, OutboundInput } from "../lib/types";

export function OutboundsPage({
  outbounds,
  capabilities,
  busy,
  onCreate,
  onUpdate,
  onDelete
}: {
  outbounds: Outbound[];
  capabilities: CapabilityMap | null;
  busy: boolean;
  onCreate: (input: OutboundInput) => void;
  onUpdate: (id: number, input: OutboundInput) => void;
  onDelete: (id: number) => void;
}) {
  const [query, setQuery] = useState("");
  const [editing, setEditing] = useState<Outbound | null>(null);
  const [drawerOpen, setDrawerOpen] = useState(false);

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return outbounds;
    return outbounds.filter((outbound) => {
      return [outbound.tag, outbound.protocol, outbound.transport, outbound.security, outbound.address ?? ""]
        .join(" ")
        .toLowerCase()
        .includes(needle);
    });
  }, [outbounds, query]);

  const openEditor = (outbound: Outbound | null) => {
    setEditing(outbound);
    setDrawerOpen(true);
  };

  return (
    <div className="page">
      <div className="page-title">
        <h1>Outbounds</h1>
        <p>Structured outbound definitions with protocol-aware destination, transport, and security controls.</p>
      </div>

      <section className="work-panel">
        <div className="panel-toolbar">
          <SearchBar value={query} onChange={setQuery} />
          <Button variant="primary" icon={<Plus size={16} />} onClick={() => openEditor(null)} disabled={busy}>
            New Outbound
          </Button>
        </div>
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Tag</th>
                <th>Protocol</th>
                <th>Transport</th>
                <th>Security</th>
                <th>Runtime Summary</th>
                <th>Status</th>
              </tr>
            </thead>
            <tbody>
              {filtered.map((outbound) => {
                return (
                  <tr key={outbound.id}>
                    <td>
                      <button className="link-cell" onClick={() => openEditor(outbound)} type="button">
                        {outbound.tag}
                      </button>
                      <small>{outbound.address ? `${outbound.address}:${outbound.port ?? "—"}` : outbound.protocol}</small>
                    </td>
                    <td>{outbound.protocol}</td>
                    <td>{outbound.transport}</td>
                    <td>
                      <div className="table-chips">
                        <Badge tone={outbound.security === "none" ? "gray" : "cyan"}>{outbound.security}</Badge>
                      </div>
                    </td>
                    <td>{outbound.address ? `${outbound.address}:${outbound.port ?? "—"}` : "direct"}</td>
                    <td>
                      <Badge tone={outbound.enabled ? "green" : "gray"}>{outbound.enabled ? "enabled" : "disabled"}</Badge>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          {filtered.length === 0 ? <div className="empty">No outbounds match the current view.</div> : null}
        </div>
      </section>

      {drawerOpen ? (
        <OutboundDrawer
          editing={editing}
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
