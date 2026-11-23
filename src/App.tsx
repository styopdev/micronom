
import { useEffect, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  Boxes,
  Code2,
  Plus,
  Play,
  Square,
  LoaderCircle,
  Search,
  Settings,
  X,
  Server,
  Cpu,
  Network,
  Trash2,
} from "lucide-react";
import AiAssistantPanel from "./components/AiAssistantPanel";
import ServiceConnectionsPanel from "./components/ServiceConnectionsPanel";
import ServiceCreationModal from "./components/ServiceCreationModal";
import ServiceDatabasePanel from "./components/ServiceDatabasePanel";
import Sidebar from "./components/Sidebar";
import { brokers, databaseOptions, languages, methods } from "./data";
import type {
  AiSuggestion,
  CommandExecution,
  Connection,
  DatabaseType,
  MessageBrokerType,
  Service,
  ServiceActionProgress,
} from "./types";
import "./App.css";

type StoredWorkspace = {
  version: 1;
  services: Service[];
  connections: Connection[];
  activeServiceId: number | null;
};

const workspaceStorageKey = "micronom.workspace";

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function isSafeInteger(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value);
}

function isDatabaseType(value: unknown): value is DatabaseType {
  return databaseOptions.some((database) => database.id === value);
}

function isService(value: unknown): value is Service {
  return (
    isRecord(value) &&
    isSafeInteger(value.id) &&
    typeof value.name === "string" &&
    typeof value.language === "string" &&
    typeof value.framework === "string" &&
    value.status === "Created" &&
    (value.database === undefined ||
      value.database === null ||
      isDatabaseType(value.database))
  );
}

function isConnection(value: unknown): value is Connection {
  return (
    isRecord(value) &&
    isSafeInteger(value.id) &&
    isSafeInteger(value.sourceId) &&
    isSafeInteger(value.targetId) &&
    typeof value.method === "string" &&
    methods.some((method) => method.name === value.method) &&
    (typeof value.broker === "string" || value.broker === null)
  );
}

function isStoredWorkspace(value: unknown): value is StoredWorkspace {
  if (
    !isRecord(value) ||
    value.version !== 1 ||
    !Array.isArray(value.services) ||
    !value.services.every(isService) ||
    !Array.isArray(value.connections) ||
    !value.connections.every(isConnection)
  ) {
    return false;
  }

  const services = value.services.filter(isService);
  const connections = value.connections.filter(isConnection);
  if (
    services.length !== value.services.length ||
    connections.length !== value.connections.length
  ) {
    return false;
  }

  const serviceIds = new Set(services.map((service) => service.id));
  const connectionIds = new Set(connections.map((connection) => connection.id));
  const serviceIdsAreUnique = serviceIds.size === services.length;
  const connectionIdsAreUnique = connectionIds.size === connections.length;
  const connectionsHaveValidServices = connections.every(
    (connection) =>
      connection.sourceId !== connection.targetId &&
      serviceIds.has(connection.sourceId) &&
      serviceIds.has(connection.targetId)
  );

  return (
    serviceIdsAreUnique &&
    connectionIdsAreUnique &&
    connectionsHaveValidServices &&
    (value.activeServiceId === null ||
      (isSafeInteger(value.activeServiceId) &&
        serviceIds.has(value.activeServiceId)))
  );
}

function saveWorkspace(workspace: StoredWorkspace): string | null {
  try {
    window.localStorage.setItem(workspaceStorageKey, JSON.stringify(workspace));
    return null;
  } catch (error) {
    return `Could not save workspace: ${String(error)}`;
  }
}

function readStoredWorkspace(): {
  workspace: StoredWorkspace;
  hasSavedWorkspace: boolean;
  error: string | null;
} {
  try {
    const saved = window.localStorage.getItem(workspaceStorageKey);
    if (saved === null) {
      return {
        workspace: {
          version: 1,
          services: [],
          connections: [],
          activeServiceId: null,
        },
        hasSavedWorkspace: false,
        error: null,
      };
    }

    const workspace: unknown = JSON.parse(saved);
    if (!isStoredWorkspace(workspace)) {
      throw new Error("Saved workspace data is invalid.");
    }

    return { workspace, hasSavedWorkspace: true, error: null };
  } catch (error) {
    return {
      workspace: {
        version: 1,
        services: [],
        connections: [],
        activeServiceId: null,
      },
      hasSavedWorkspace: true,
      error: `Could not load saved workspace: ${String(error)}`,
    };
  }
}

function App() {
  const [savedWorkspace] = useState(readStoredWorkspace);
  const [services, setServices] = useState(savedWorkspace.workspace.services);
  const [connections, setConnections] = useState(
    savedWorkspace.workspace.connections
  );
  const [workspaceHydrated, setWorkspaceHydrated] = useState(
    savedWorkspace.hasSavedWorkspace || !isTauri()
  );
  const [workspaceStorageError, setWorkspaceStorageError] = useState(
    savedWorkspace.error
  );

  const [modalOpen, setModalOpen] = useState(false);
  const [serviceName, setServiceName] = useState("");
  const [language, setLanguage] = useState("Node.js");
  const [activeService, setActiveService] = useState<number | null>(
    savedWorkspace.workspace.activeServiceId
  );
  const activeServiceRef = useRef<number | null>(
    savedWorkspace.workspace.activeServiceId
  );

  const [aiModel, setAiModel] = useState("qwen2.5-coder:7b");
  const [aiPrompt, setAiPrompt] = useState("");
  const [aiSuggestion, setAiSuggestion] = useState<AiSuggestion | null>(null);
  const [aiExecution, setAiExecution] = useState<CommandExecution | null>(null);
  const [aiError, setAiError] = useState("");
  const [aiBusy, setAiBusy] = useState<"suggesting" | "running" | null>(null);

  const selectedLanguage = languages.find(
    (item) => item.name === language
  )!;

  const selectedService = services.find(
    (service) => service.id === activeService
  );

  const [bulkLoading, setBulkLoading] = useState(false);
  const [bulkProgress, setBulkProgress] = useState<ServiceActionProgress[]>([]);

  function persistWorkspaceState(
    nextServices = services,
    nextConnections = connections,
    nextActiveServiceId = activeService
  ) {
    if (workspaceStorageError) return;

    const error = saveWorkspace({
      version: 1,
      services: nextServices,
      connections: nextConnections,
      activeServiceId: nextActiveServiceId,
    });
    if (error) setWorkspaceStorageError(error);
  }

  function selectService(serviceId: number | null) {
    setActiveService(serviceId);
    activeServiceRef.current = serviceId;
  }

  useEffect(() => {
    if (savedWorkspace.hasSavedWorkspace || !isTauri()) return;

    let cancelled = false;
    Promise.all([
      invoke<{
        name: string;
        language: string;
        framework: string;
        database: DatabaseType | null;
      }[]>("list_service_folders"),
      invoke<{
        id: number;
        sourceService: string;
        targetService: string;
        method: "messaging" | "grpc";
        broker: MessageBrokerType | null;
      }[]>("list_service_connections"),
    ])
      .then(([existingServices, existingConnections]) => {
        if (cancelled) return;

        const restoredServices: Service[] = existingServices.map(
          (service, index) => ({
            ...service,
            id: index + 1,
            status: "Created",
            database: service.database,
          })
        );
        const serviceIds = new Map(
          restoredServices.map((service) => [service.name, service.id])
        );
        const restoredConnections: Connection[] = existingConnections.map(
          (connection) => {
            const sourceId = serviceIds.get(connection.sourceService);
            const targetId = serviceIds.get(connection.targetService);
            const broker = connection.broker
              ? brokers.find((item) => item.id === connection.broker)
              : undefined;
            const isMessaging = connection.method === "messaging";
            if (
              !isSafeInteger(connection.id) ||
              sourceId === undefined ||
              targetId === undefined ||
              (isMessaging && !broker) ||
              (!isMessaging && connection.broker !== null)
            ) {
              throw new Error("Saved service connection data is invalid.");
            }

            return {
              id: connection.id,
              sourceId,
              targetId,
              method: isMessaging ? "Messaging" : "gRPC",
              broker: isMessaging ? broker?.name ?? null : null,
            };
          }
        );
        setServices(restoredServices);
        setConnections(restoredConnections);
        const saveError = saveWorkspace({
          version: 1,
          services: restoredServices,
          connections: restoredConnections,
          activeServiceId: null,
        });
        if (saveError) setWorkspaceStorageError(saveError);
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setWorkspaceStorageError(
            `Could not restore existing services: ${String(error)}`
          );
        }
      })
      .finally(() => {
        if (!cancelled) setWorkspaceHydrated(true);
      });

    return () => {
      cancelled = true;
    };
  }, [savedWorkspace.hasSavedWorkspace]);

  async function askAiForCommand() {
    if (!selectedService || !aiPrompt.trim()) return;
    const serviceId = selectedService.id;

    setAiBusy("suggesting");
    setAiError("");
    setAiSuggestion(null);
    setAiExecution(null);

    try {
      const suggestion = await invoke<AiSuggestion>("suggest_service_command", {
        model: aiModel.trim(),
        prompt: aiPrompt.trim(),
        language: selectedService.language,
        framework: selectedService.framework,
      });
      if (activeServiceRef.current === serviceId) setAiSuggestion(suggestion);
    } catch (error) {
      if (activeServiceRef.current === serviceId) setAiError(String(error));
    } finally {
      setAiBusy(null);
    }
  }

  async function runSuggestedCommand() {
    if (!selectedService || !aiSuggestion?.commandId) return;
    const serviceId = selectedService.id;

    setAiBusy("running");
    setAiError("");
    setAiExecution(null);

    try {
      const result = await invoke<CommandExecution>("execute_service_command", {
        serviceName: selectedService.name,
        commandId: aiSuggestion.commandId,
        language: selectedService.language,
      });
      if (activeServiceRef.current === serviceId) setAiExecution(result);
    } catch (error) {
      if (activeServiceRef.current === serviceId) setAiError(String(error));
    } finally {
      setAiBusy(null);
    }
  }

  function handleDatabaseConfigured(serviceId: number, database: DatabaseType) {
    const nextServices = services.map((service) =>
      service.id === serviceId ? { ...service, database } : service
    );
    setServices(nextServices);
    persistWorkspaceState(nextServices, connections, activeService);
  }

  function handleConnectionCreated(connection: Connection) {
    const nextConnections = [...connections, connection];
    setConnections(nextConnections);
    persistWorkspaceState(services, nextConnections, activeService);
  }

  async function handleBulkAction(action: "run" | "stop") {
    setBulkLoading(true);
    setBulkProgress([]);
    let unlisten: (() => void) | undefined;

    try {
      if (isTauri()) {
        unlisten = await listen<ServiceActionProgress>(
          "service-action-progress",
          ({ payload }) => {
            setBulkProgress((current) => [...current.slice(-199), payload]);
          }
        );
      }

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
      unlisten?.();
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
        id: services.reduce(
          (highestId, existing) => Math.max(highestId, existing.id),
          0
        ) + 1,
        name,
        language,
        framework: selectedLanguage.framework,
        status: "Created",
        database: null,
      };

      const nextServices = [...services, service];
      setServices(nextServices);
      selectService(service.id);
      persistWorkspaceState(nextServices, connections, service.id);
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
      const nextServices = services.filter(
        (service) => service.id !== removedId
      );
      const nextConnections = connections.filter(
        (connection) =>
          connection.sourceId !== removedId &&
          connection.targetId !== removedId
      );

      setServices(nextServices);
      setConnections(nextConnections);
      selectService(null);
      persistWorkspaceState(nextServices, nextConnections, null);
    } catch (error) {
      alert(`Could not remove microservice:\n${String(error)}`);
    }
  }

  return (
    <div className="app-shell">
      <Sidebar
        serviceCount={services.length}
        connectionCount={connections.length}
      />

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
          {workspaceStorageError && (
            <div className="workspace-storage-error" role="alert">
              {workspaceStorageError}
            </div>
          )}

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
          {bulkProgress.length > 0 && (
            <div className="bulk-progress" role="status" aria-live="polite">
              {bulkProgress.map((entry, index) => (
                <p key={`${entry.name}-${index}`}>
                  <strong>{entry.name}:</strong> {entry.message}
                </p>
              ))}
            </div>
          )}
          <section className="welcome-section">
            <div>
              <div className="eyebrow">YOUR DEVELOPMENT WORKSPACE</div>
              <p>
                Design, build and orchestrate your microservices in one place.
              </p>
            </div>
            <button
              className="primary-button"
              onClick={() => setModalOpen(true)}
              disabled={!workspaceHydrated}
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
              </div>
            </div>

            {!workspaceHydrated ? (
              <div className="empty-state">
                <LoaderCircle size={26} className="spin" />
                <h3>Restoring workspace</h3>
                <p>Loading your saved microservices.</p>
              </div>
            ) : services.length === 0 ? (
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
                    onClick={() => {
                    selectService(service.id);
                    persistWorkspaceState(services, connections, service.id);
                      setAiSuggestion(null);
                      setAiExecution(null);
                      setAiError("");
                    }}
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
                  onClick={() => {
                    selectService(null);
                    persistWorkspaceState(services, connections, null);
                  }}
                  title="Close service"
                >
                  <X size={18} />
                </button>
              </div>
              </div>

              <ServiceConnectionsPanel
                service={selectedService}
                services={services}
                connections={connections}
                onConnectionCreated={handleConnectionCreated}
              />
              <ServiceDatabasePanel
                service={selectedService}
                onDatabaseConfigured={handleDatabaseConfigured}
              />
              <AiAssistantPanel
                model={aiModel}
                prompt={aiPrompt}
                suggestion={aiSuggestion}
                execution={aiExecution}
                error={aiError}
                busy={aiBusy}
                onModelChange={(value) => {
                  setAiModel(value);
                  setAiSuggestion(null);
                  setAiExecution(null);
                  setAiError("");
                }}
                onPromptChange={(value) => {
                  setAiPrompt(value);
                  setAiSuggestion(null);
                  setAiExecution(null);
                  setAiError("");
                }}
                onAsk={askAiForCommand}
                onRun={runSuggestedCommand}
              />
            </section>
          )}
        </div>
      </main>

      <ServiceCreationModal
        open={modalOpen}
        serviceName={serviceName}
        language={language}
        languages={languages}
        onServiceNameChange={setServiceName}
        onLanguageChange={setLanguage}
        onCreate={createService}
        onClose={() => setModalOpen(false)}
      />
    </div>
  );
}

export default App;
