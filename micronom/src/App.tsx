
import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Activity,
  Boxes,
  Code2,
  Database,
  Folder,
  LayoutDashboard,
  Plus,
  Search,
  Settings,
  Terminal,
  Workflow,
  X,
  Server,
  Cpu,
} from "lucide-react";
import "./App.css";

type Service = {
  id: number;
  name: string;
  language: string;
  framework: string;
  status: "Created";
};

const languages = [
  { name: "Node.js", framework: "Express", icon: "JS" },
  { name: "Python", framework: "FastAPI", icon: "PY" },
  { name: "Go", framework: "Gin", icon: "GO" },
  { name: "Rust", framework: "Axum", icon: "RS" },
];

function App() {
  const [services, setServices] = useState<Service[]>([]);
  const [modalOpen, setModalOpen] = useState(false);
  const [serviceName, setServiceName] = useState("");
  const [language, setLanguage] = useState("Node.js");
  const [activeService, setActiveService] = useState<number | null>(null);

  const selectedLanguage = languages.find(
    (item) => item.name === language
  )!;

  async function createService() {
    const name = serviceName.trim().toLowerCase().replace(/\s+/g, "-");

    if (!name) return;

    try {
      const folderPath = await invoke<string>("create_service_folder", {
        name,
      });

      const service: Service = {
        id: Date.now(),
        name,
        language,
        framework: selectedLanguage.framework,
        status: "Created",
      };

      setServices((previous) => [...previous, service]);
      setActiveService(service.id);
      setServiceName("");
      setModalOpen(false);

      console.log("Service created at:", folderPath);
    } catch (error) {
      console.error("Failed to create service:", error);
      alert(String(error));
    }
  }

  const selectedService = services.find(
    (service) => service.id === activeService
  );

  return (
    <div className="app-shell">
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
          <span className="nav-count">{services.length}</span>
        </button>

        <button className="nav-item">
          <Database size={17} />
          Databases
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

      <main className="main-content">
        <header className="topbar">
          <div className="breadcrumb">
            <span>Workspace</span>
            <span className="breadcrumb-separator">/</span>
            <strong>Dashboard</strong>
          </div>
          <div className="topbar-right">
            <div className="local-badge">
              <span className="status-dot" />
              Local environment
            </div>
            <button className="icon-button" title="Settings">
              <Settings size={18} />
            </button>
          </div>
        </header>

        <div className="page-content">
          <section className="welcome-section">
            <div>
              <div className="eyebrow">YOUR DEVELOPMENT WORKSPACE</div>
              <h1>Welcome to Micronom.</h1>
              <p>
                Design, build and orchestrate your microservices in one place.
              </p>
            </div>
            <button
              className="primary-button"
              onClick={() => setModalOpen(true)}
            >
              <Plus size={18} />
              Add microservice
            </button>
          </section>

          <section className="stats-grid">
            <div className="stat-card">
              <div className="stat-top">
                <span>MICROSERVICES</span>
                <Server size={18} />
              </div>
              <div className="stat-value">{services.length}</div>
              <div className="stat-description">Created services</div>
            </div>

            <div className="stat-card">
              <div className="stat-top">
                <span>DATABASES</span>
                <Database size={18} />
              </div>
              <div className="stat-value">0</div>
              <div className="stat-description">Configured databases</div>
            </div>

            <div className="stat-card">
              <div className="stat-top">
                <span>RUNTIME</span>
                <Cpu size={18} />
              </div>
              <div className="stat-value">Local</div>
              <div className="stat-description">Docker environment</div>
            </div>
          </section>

          <section className="services-section">
            <div className="section-heading">
              <div>
                <h2>Microservices</h2>
                <p>Your project's individual services.</p>
              </div>
              <div className="service-actions">
                <div className="search-box">
                  <Search size={15} />
                  <span>Search services</span>
                </div>
                <button
                  className="secondary-button"
                  onClick={() => setModalOpen(true)}
                >
                  <Plus size={16} />
                  New service
                </button>
              </div>
            </div>

            {services.length === 0 ? (
              <div className="empty-state">
                <div className="empty-icon">
                  <Boxes size={30} />
                </div>
                <h3>No microservices yet</h3>
                <p>
                  Your architecture starts with a single service.
                  Create one to get started.
                </p>
                <button
                  className="primary-button"
                  onClick={() => setModalOpen(true)}
                >
                  <Plus size={17} />
                  Create your first service
                </button>
              </div>
            ) : (
              <div className="service-grid">
                {services.map((service) => (
                  <button
                    className={`service-card ${
                      activeService === service.id ? "selected" : ""
                    }`}
                    key={service.id}
                    onClick={() => setActiveService(service.id)}
                  >
                    <div className="service-card-top">
                      <div className="service-icon">
                        <Code2 size={20} />
                      </div>
                      <span className="created-label">CREATED</span>
                    </div>
                    <h3>{service.name}</h3>
                    <p>
                      {service.language} · {service.framework}
                    </p>
                  </button>
                ))}
              </div>
            )}
          </section>

          {selectedService && (
            <section className="selected-service">
              <div className="section-heading">
                <div>
                  <div className="eyebrow">SERVICE WORKSPACE</div>
                  <h2>{selectedService.name}</h2>
                  <p>
                    {selectedService.language} · {selectedService.framework}
                  </p>
                </div>
                <button
                  className="icon-button"
                  onClick={() => setActiveService(null)}
                  title="Close service"
                >
                  <X size={18} />
                </button>
              </div>
              <div className="workspace-placeholder">
                <Folder size={26} />
                <h3>Service workspace</h3>
                <p>
                  Code editor, AI prompt, database configuration and logs
                  will live here.
                </p>
              </div>
            </section>
          )}
        </div>
      </main>

      {modalOpen && (
        <div
          className="modal-overlay"
          onClick={() => setModalOpen(false)}
        >
          <div
            className="modal"
            onClick={(event) => event.stopPropagation()}
          >
            <div className="modal-header">
              <div>
                <h2>Create microservice</h2>
                <p>Configure your new service.</p>
              </div>
              <button
                className="icon-button"
                onClick={() => setModalOpen(false)}
              >
                <X size={19} />
              </button>
            </div>

            <label className="field-label">SERVICE NAME</label>
            <input
              className="text-input"
              placeholder="e.g. user-service"
              value={serviceName}
              onChange={(event) => setServiceName(event.target.value)}
              autoFocus
            />

            <label className="field-label language-label">
              PROGRAMMING LANGUAGE
            </label>

            <div className="language-grid">
              {languages.map((item) => (
                <button
                  key={item.name}
                  className={`language-option ${
                    language === item.name ? "selected" : ""
                  }`}
                  onClick={() => setLanguage(item.name)}
                >
                  <span className="language-icon">{item.icon}</span>
                  <span className="language-info">
                    <strong>{item.name}</strong>
                    <small>{item.framework}</small>
                  </span>
                  <span className="language-radio">
                    {language === item.name ? "✓" : ""}
                  </span>
                </button>
              ))}
            </div>

            <div className="modal-footer">
              <button
                className="secondary-button"
                onClick={() => setModalOpen(false)}
              >
                Cancel
              </button>
              <button
                className="primary-button"
                onClick={createService}
                disabled={!serviceName.trim()}
              >
                <Plus size={17} />
                Create service
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export default App;
