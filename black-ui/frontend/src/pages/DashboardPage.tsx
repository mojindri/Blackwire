import { Activity, CircleGauge, Database, Users } from "lucide-react";
import type { ReactNode } from "react";
import type { AppData } from "../lib/types";
import { formatBytes } from "../lib/format";

export function DashboardPage({ data }: { data: AppData }) {
  const cachedUserUpload = data.users.reduce((sum, user) => sum + user.uploadBytes, 0);
  const cachedUserDownload = data.users.reduce((sum, user) => sum + user.downloadBytes, 0);
  const liveInboundUpload = data.traffic.inbounds.reduce((sum, inbound) => sum + inbound.uploadBytes, 0);
  const liveInboundDownload = data.traffic.inbounds.reduce((sum, inbound) => sum + inbound.downloadBytes, 0);
  const totalUpload = Math.max(cachedUserUpload, liveInboundUpload);
  const totalDownload = Math.max(cachedUserDownload, liveInboundDownload);
  return (
    <div className="page">
      <div className="page-title">
        <h1>Dashboard</h1>
        <p>Runtime health, traffic, and Blackwire-native panel shape.</p>
      </div>
      <div className="metric-grid">
        <Metric icon={<Users />} label="Active users" value={`${data.status?.activeUsers ?? 0}`} sub={`${data.status?.users ?? 0} total`} />
        <Metric icon={<CircleGauge />} label="Traffic" value={formatBytes(totalUpload + totalDownload)} sub={`${formatBytes(totalUpload)} up · ${formatBytes(totalDownload)} down`} />
        <Metric icon={<Database />} label="Revision" value={`${data.status?.activeRevision ?? "—"}`} sub={`desired ${data.status?.desiredRevision ?? "—"}`} />
        <Metric icon={<Activity />} label="Runtime" value={data.status?.grpcReachable ? "Live" : "Offline"} sub={data.status?.activationState ?? "loading"} />
      </div>
      <section className="work-panel split-panel">
        <div>
          <h2>Traffic by inbound</h2>
          <div className="mini-list">
            {data.traffic.inbounds.map((row) => (
              <div key={row.tag}>
                <span>{row.tag}</span>
                <strong>{formatBytes(row.uploadBytes + row.downloadBytes)}</strong>
              </div>
            ))}
            {data.traffic.inbounds.length === 0 ? <p>No live inbound traffic available.</p> : null}
          </div>
        </div>
        <div>
          <h2>Control plane</h2>
          <div className="mini-list">
            <div><span>MySQL</span><strong>{data.status?.databaseConnected ? "Connected" : "Unavailable"}</strong></div>
            <div><span>Schema</span><strong>v{data.status?.schemaVersion ?? "—"}</strong></div>
            <div><span>Activation</span><strong>{data.status?.activationState ?? "Loading"}</strong></div>
          </div>
        </div>
      </section>
    </div>
  );
}

function Metric({ icon, label, value, sub }: { icon: ReactNode; label: string; value: string; sub: string }) {
  return (
    <section className="metric">
      <span className="metric-icon">{icon}</span>
      <span>{label}</span>
      <strong>{value}</strong>
      <small>{sub}</small>
    </section>
  );
}
