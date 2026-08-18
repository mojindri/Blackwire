import { Cable, Gauge, Route, ServerCog, Settings, SlidersHorizontal, Users, Waypoints } from "lucide-react";
import packageInfo from "../../../package.json";
import type { PageKey } from "../../lib/types";

const items: Array<{ key: PageKey; label: string; icon: typeof Gauge }> = [
  { key: "dashboard", label: "Dashboard", icon: Gauge },
  { key: "users", label: "Users", icon: Users },
  { key: "inbounds", label: "Inbounds", icon: Waypoints },
  { key: "outbounds", label: "Outbounds", icon: Route },
  { key: "sections", label: "Routing & DNS", icon: SlidersHorizontal },
  { key: "service", label: "Runtime", icon: ServerCog },
  { key: "settings", label: "Settings", icon: Settings }
];

export function AppSidebar({ page, onPage }: { page: PageKey; onPage: (page: PageKey) => void }) {
  return (
    <aside className="sidebar">
      <div className="brand">
        <Cable size={24} />
        <div className="brand-text">
          <span>Blackwire</span>
          <small>v{packageInfo.version}</small>
        </div>
      </div>
      <nav>
        {items.map((item) => {
          const Icon = item.icon;
          return (
            <button
              key={item.key}
              className={page === item.key ? "active" : ""}
              onClick={() => onPage(item.key)}
              type="button"
            >
              <Icon size={18} />
              {item.label}
            </button>
          );
        })}
      </nav>
      <div className="sidebar-foot">
        <span>Blackwire Panel</span>
        <small>black-ui</small>
      </div>
    </aside>
  );
}
