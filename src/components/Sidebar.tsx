import {
  Activity,
  Boxes,
  Database,
  LayoutDashboard,
  Network,
  Settings,
  Terminal,
  Workflow,
} from "lucide-react";

type SidebarProps = {
  serviceCount: number;
  connectionCount: number;
};

export default function Sidebar({
  serviceCount,
  connectionCount,
}: SidebarProps) {
  return (
    <aside className="sidebar">
      <div className="brand">
        <div className="brand-icon">
          <Workflow size={22} />
        </div>
        <div>
          <div className="brand-name">micronom</div>
          <div className="brand-subtitle">MICROSERVICES STUDIO</div>
        </div>
      </div>

      <div className="workspace-label">WORKSPACE</div>

      <button className="nav-item active">
        <LayoutDashboard size={17} />
        Dashboard
      </button>

      <button className="nav-item">
        <Boxes size={17} />
        Microservices
        <span className="nav-count">{serviceCount}</span>
      </button>

      <button className="nav-item">
        <Database size={17} />
        Databases
      </button>

      <button className="nav-item">
        <Network size={17} />
        Connections
        <span className="nav-count">{connectionCount}</span>
      </button>

      <button className="nav-item">
        <Activity size={17} />
        Monitoring
      </button>

      <div className="sidebar-bottom">
        <div className="workspace-label">SYSTEM</div>
        <button className="nav-item">
          <Terminal size={17} />
          Logs
        </button>
        <button className="nav-item">
          <Settings size={17} />
          Settings
        </button>

        <div className="engine-status">
          <span className="status-dot" />
          <div>
            <div className="engine-title">Micronom Engine</div>
            <div className="engine-subtitle">Desktop environment</div>
          </div>
        </div>
      </div>
    </aside>
  );
}
