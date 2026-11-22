
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
  Play,
  Square,
  LoaderCircle,
  Search,
  Settings,
  Terminal,
  Workflow,
  X,
  Server,
  Cpu,
  Network,
  RadioTower,
  Globe,
  Webhook,
  ArrowRight,
  MessageSquare,
  Trash2,
} from "lucide-react";
import "./App.css";

type Service = {
  id: number;
  name: string;
  language: string;
  framework: string;
  status: "Created";
};

type ConnectionMethod = "Messaging" | "REST API" | "gRPC" | "Webhooks";

type Connection = {
  id: number;
  sourceId: number;
  targetId: number;
  method: ConnectionMethod;
  broker: string | null;
};

const languages = [
  { name: "Node.js", framework: "Express", icon: "JS" },
  { name: "Python", framework: "FastAPI", icon: "PY" },
  { name: "Go", framework: "Gin", icon: "GO" },
  { name: "Rust", framework: "Axum", icon: "RS" },
];

const methods: {
  name: ConnectionMethod;
  description: string;
  icon: typeof Network;
}[] = [
  {
    name: "Messaging",
    description: "Asynchronous communication through message brokers.",
    icon: RadioTower,
  },
  {
    name: "REST API",
    description: "HTTP requests between microservices.",
    icon: Globe,
  },
  {
    name: "gRPC",
    description: "Fast, strongly typed service communication.",
    icon: Network,
  },
  {
    name: "Webhooks",
    description: "Send event notifications between services.",
    icon: Webhook,
  },
];

const brokers = [
  {
    name: "RabbitMQ",
    description: "Reliable message queues and routing.",
    icon: "RMQ",
  },
  {
    name: "Apache Kafka",
    description: "Distributed event streaming.",
    icon: "KFK",
  },
  {
    name: "Redis Streams",
    description: "Lightweight stream-based messaging.",
    icon: "RED",
  },
];

function App() {
  const [services, setServices] = useState<Service[]>([]);
  const [connections, setConnections] = useState<Connection[]>([]);

  const [modalOpen, setModalOpen] = useState(false);
  const [connectionModalOpen, setConnectionModalOpen] = useState(false);

  const [serviceName, setServiceName] = useState("");
  const [language, setLanguage] = useState("Node.js");
  const [activeService, setActiveService] = useState<number | null>(null);

  const [connectionMethod, setConnectionMethod] =
    useState<ConnectionMethod>("Messaging");
  const [selectedBroker, setSelectedBroker] = useState("RabbitMQ");
  const [targetServiceId, setTargetServiceId] = useState<number | "">("");

  const [connectionError, setConnectionError] = useState("");

  const selectedLanguage = languages.find(
    (item) => item.name === language
  )!;

  const selectedService = services.find(
    (service) => service.id === activeService
  );

  const targetServices = services.filter(
    (service) => service.id !== activeService
  );

  const serviceConnections = connections.filter(
    (connection) =>
      connection.sourceId === activeService ||
      connection.targetId === activeService
  );

  const [bulkLoading, setBulkLoading] = useState(false);

  async function handleBulkAction(action: "run" | "stop") {
    setBulkLoading(true);

    try {
      const results = await invoke<
        { name: string; success: boolean; message: string }[]
      >(
        action === "run" ? "run_all_services" : "stop_all_services"
      );

      const failures = results.filter((result) => !result.success);

      if (failures.length > 0) {
        alert(
          failures.map(
            (result) => `${result.name}: ${result.message}`
          ).join("\n\n")
        );
      } else {
        alert(
          action === "run"
            ? "All microservices started successfully."
            : "All microservices stopped successfully."
        );
      }
    } catch (error) {
      alert(`Docker operation failed: ${String(error)}`);
    } finally {
      setBulkLoading(false);
    }
  }

  async function createService() {
    const name = serviceName.trim().toLowerCase().replace(/\s+/g, "-");

    if (!name) return;

    try {
      const folderPath = await invoke<string>("create_service_folder", {
        name,
        language,
        framework: selectedLanguage.framework,
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

  async function removeService() {
    if (!selectedService) return;

    const confirmed = window.confirm(
      `Permanently delete "${selectedService.name}"?\n\n` +
      `This will delete its files and folders from your app directory. ` +
      `This action cannot be undone.`
    );

    if (!confirmed) return;

    try {
      await invoke("remove_service_folder", {
        name: selectedService.name,
      });

      const removedId = selectedService.id;

      setServices((previous) =>
        previous.filter((service) => service.id !== removedId)
      );

      setConnections((previous) =>
        previous.filter(
          (connection) =>
            connection.sourceId !== removedId &&
            connection.targetId !== removedId
        )
      );

      setActiveService(null);
    } catch (error) {
      alert(`Could not remove microservice:\n${String(error)}`);
    }
  }

  function openConnectionModal() {
    setConnectionMethod("Messaging");
    setSelectedBroker("RabbitMQ");
    setTargetServiceId("");
    setConnectionError("");
    setConnectionModalOpen(true);
  }

  function createConnection() {
    if (activeService === null || targetServiceId === "") {
      setConnectionError("Please select a destination microservice.");
      return;
    }

    if (activeService === targetServiceId) {
      setConnectionError("A service cannot connect to itself.");
      return;
    }

    const alreadyExists = connections.some(
      (connection) =>
        connection.sourceId === activeService &&
        connection.targetId === targetServiceId &&
        connection.method === connectionMethod &&
        connection.broker ===
          (connectionMethod === "Messaging" ? selectedBroker : null)
    );

    if (alreadyExists) {
      setConnectionError("This connection already exists.");
      return;
    }

    const newConnection: Connection = {
      id: Date.now(),
      sourceId: activeService,
      targetId: targetServiceId,
      method: connectionMethod,
      broker:
        connectionMethod === "Messaging" ? selectedBroker : null,
    };

    setConnections((previous) => [...previous, newConnection]);
    setConnectionModalOpen(false);
    setConnectionError("");
  }

  function getServiceName(id: number) {
    return services.find((service) => service.id === id)?.name ?? "Unknown";
  }

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
          <Network size={17} />
          Connections
          <span className="nav-count">{connections.length}</span>
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
          <div className="bulk-actions">
            <button
              className="run-all-button"
              onClick={() => handleBulkAction("run")}
              disabled={bulkLoading}
            >
              {bulkLoading ? (
                <LoaderCircle size={16} className="spin" />
              ) : (
                <Play size={16} />
              )}
              Run all
            </button>

            <button
              className="stop-all-button"
              onClick={() => handleBulkAction("stop")}
              disabled={bulkLoading}
            >
              <Square size={15} />
              Shut down all
            </button>
          </div>
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
                <span>CONNECTIONS</span>
                <Network size={18} />
              </div>
              <div className="stat-value">{connections.length}</div>
              <div className="stat-description">Configured connections</div>
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

              <div className="service-header-actions">
                <button
                  className="danger-button"
                  onClick={removeService}
                >
                  <Trash2 size={16} />
                  Remove microservice
                </button>

                <button
                  className="icon-button"
                  onClick={() => setActiveService(null)}
                  title="Close service"
                >
                  <X size={18} />
                </button>
              </div>
              </div>

              <div className="connection-panel">
                <div className="connection-panel-heading">
                  <div>
                    <h3>Service connections</h3>
                    <p>Connect this microservice to other services.</p>
                  </div>
                  <button
                    className="primary-button"
                    onClick={openConnectionModal}
                    disabled={services.length < 2}
                    title={
                      services.length < 2
                        ? "Create another microservice first"
                        : "Create a connection"
                    }
                  >
                    <Plus size={17} />
                    Connect
                  </button>
                </div>

                {serviceConnections.length === 0 ? (
                  <div className="connection-empty">
                    <div className="empty-icon">
                      <Network size={26} />
                    </div>
                    <h3>No connections yet</h3>
                    <p>
                      Connect {selectedService.name} to another microservice
                      using messaging, REST, gRPC or webhooks.
                    </p>
                    {services.length < 2 && (
                      <small>Create at least two microservices to connect them.</small>
                    )}
                  </div>
                ) : (
                  <div className="connection-list">
                    {serviceConnections.map((connection) => {
                      const isOutgoing =
                        connection.sourceId === selectedService.id;

                      return (
                        <div className="connection-item" key={connection.id}>
                          <div className="connection-item-icon">
                            {connection.method === "Messaging" ? (
                              <RadioTower size={19} />
                            ) : connection.method === "REST API" ? (
                              <Globe size={19} />
                            ) : connection.method === "gRPC" ? (
                              <Network size={19} />
                            ) : (
                              <Webhook size={19} />
                            )}
                          </div>
                          <div className="connection-item-info">
                            <strong>
                              {isOutgoing
                                ? `To ${getServiceName(connection.targetId)}`
                                : `From ${getServiceName(connection.sourceId)}`}
                            </strong>
                            <span>
                              {connection.method}
                              {connection.broker
                                ? ` · ${connection.broker}`
                                : ""}
                            </span>
                          </div>
                          <span className="connection-direction">
                            {isOutgoing ? "Outgoing" : "Incoming"}
                          </span>
                        </div>
                      );
                    })}
                  </div>
                )}
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

      {connectionModalOpen && selectedService && (
        <div
          className="modal-overlay"
          onClick={() => setConnectionModalOpen(false)}
        >
          <div
            className="modal connection-modal"
            onClick={(event) => event.stopPropagation()}
          >
            <div className="modal-header">
              <div>
                <div className="eyebrow">SERVICE CONNECTION</div>
                <h2>Connect microservice</h2>
                <p>Configure communication for {selectedService.name}.</p>
              </div>
              <button
                className="icon-button"
                onClick={() => setConnectionModalOpen(false)}
              >
                <X size={19} />
              </button>
            </div>

            <label className="field-label">1. COMMUNICATION METHOD</label>

            <div className="connection-method-grid">
              {methods.map((method) => {
                const Icon = method.icon;

                return (
                  <button
                    key={method.name}
                    className={`connection-method-option ${
                      connectionMethod === method.name ? "selected" : ""
                    }`}
                    onClick={() => {
                      setConnectionMethod(method.name);
                      setConnectionError("");
                    }}
                  >
                    <Icon size={21} />
                    <strong>{method.name}</strong>
                    <small>{method.description}</small>
                  </button>
                );
              })}
            </div>

            {connectionMethod === "Messaging" && (
              <>
                <label className="field-label connection-field-label">
                  2. SELECT MESSAGE BROKER
                </label>

                <div className="broker-grid">
                  {brokers.map((broker) => (
                    <button
                      key={broker.name}
                      className={`broker-option ${
                        selectedBroker === broker.name ? "selected" : ""
                      }`}
                      onClick={() => setSelectedBroker(broker.name)}
                    >
                      <span className="broker-icon">{broker.icon}</span>
                      <span className="broker-info">
                        <strong>{broker.name}</strong>
                        <small>{broker.description}</small>
                      </span>
                      <span className="language-radio">
                        {selectedBroker === broker.name ? "✓" : ""}
                      </span>
                    </button>
                  ))}
                </div>
              </>
            )}

            <label className="field-label connection-field-label">
              {connectionMethod === "Messaging" ? "3." : "2."} DESTINATION MICROSERVICE
            </label>

            <select
              className="text-input connection-select"
              value={targetServiceId}
              onChange={(event) => {
                setTargetServiceId(
                  event.target.value === ""
                    ? ""
                    : Number(event.target.value)
                );
                setConnectionError("");
              }}
            >
              <option value="">Select a microservice...</option>
              {targetServices.map((service) => (
                <option key={service.id} value={service.id}>
                  {service.name} — {service.language}
                </option>
              ))}
            </select>

            <div className="connection-preview">
              <div className="connection-preview-service">
                <Server size={18} />
                <span>{selectedService.name}</span>
              </div>
              <ArrowRight size={18} />
              <div className="connection-preview-service">
                <Server size={18} />
                <span>
                  {targetServiceId === ""
                    ? "Destination"
                    : getServiceName(targetServiceId)}
                </span>
              </div>
            </div>

            {connectionError && (
              <div className="connection-error">{connectionError}</div>
            )}

            <div className="connection-note">
              <MessageSquare size={16} />
              <span>
                This creates a connection configuration in Micronom.
                Docker deployment and code generation will be added next.
              </span>
            </div>

            <div className="modal-footer">
              <button
                className="secondary-button"
                onClick={() => setConnectionModalOpen(false)}
              >
                Cancel
              </button>
              <button
                className="primary-button"
                onClick={createConnection}
                disabled={targetServices.length === 0}
              >
                <Network size={17} />
                Create connection
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export default App;
