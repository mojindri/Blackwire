import { Database, LoaderCircle, LogOut, RefreshCw, ServerCog } from "lucide-react";
import { Button } from "../atoms/Button";
import { StatusDot } from "../atoms/StatusDot";
import type { Status } from "../../lib/types";

export function TopStatusStrip({
  status,
  message,
  busy,
  onRefresh,
  onLogout
}: {
  status: Status | null;
  message: string;
  busy: boolean;
  onRefresh: () => void;
  onLogout: () => void;
}) {
  return (
    <header className="top-strip">
      <div className="top-status">
        <ServerCog size={17} />
        <StatusDot tone={status?.databaseConnected ? "green" : "amber"} label={status?.databaseConnected ? "MySQL connected" : "MySQL unavailable"} />
        {busy ? <LoaderCircle size={16} className="spinner" /> : null}
        <span className="strip-sep" />
        {message ? (
          <span className="strip-message">{message}</span>
        ) : (
          <span className="revision-status"><Database size={14} /> Revision {status?.desiredRevision ?? "—"} → {status?.activeRevision ?? "—"} · {status?.activationState ?? "loading"}</span>
        )}
      </div>
      <div className="top-actions">
        <Button variant="ghost" icon={<RefreshCw size={16} className={busy ? "spinner" : ""} />} onClick={onRefresh} disabled={busy}>
          Refresh
        </Button>
        <Button variant="ghost" icon={<LogOut size={16} />} onClick={onLogout}>
          Logout
        </Button>
      </div>
    </header>
  );
}
