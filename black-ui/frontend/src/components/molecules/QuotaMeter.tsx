import { formatBytes, quotaPercent } from "../../lib/format";

export function QuotaMeter({
  upload,
  download,
  limit
}: {
  upload: number;
  download: number;
  limit: number | null;
}) {
  const pct = quotaPercent(upload, download, limit);
  const tone = pct >= 95 ? "danger" : pct >= 75 ? "warn" : "ok";
  const total = formatBytes(upload + download);

  if (!limit) {
    return (
      <div className="quota quota-unlimited">
        <div className="quota-line quota-line-unlimited">
          <span className="quota-state">Unlimited</span>
          <span>{total} used</span>
        </div>
      </div>
    );
  }

  return (
    <div className="quota">
      <div className="quota-line">
        <span className="quota-state">{pct}%</span>
        <span>{total} / {formatBytes(limit)}</span>
      </div>
      <span className="quota-track">
        <span className={`quota-fill quota-${tone}`} style={{ width: `${pct}%` }} />
      </span>
    </div>
  );
}
