import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  ArrowRight,
  Globe,
  LoaderCircle,
  MessageSquare,
  Network,
  Plus,
  RadioTower,
  Server,
  Webhook,
  X,
} from "lucide-react";
import { brokers, methods } from "../data";
import type {
  Connection,
  ConnectionMethod,
  Service,
} from "../types";

type ServiceConnectionsPanelProps = {
  service: Service;
  services: Service[];
  connections: Connection[];
  onConnectionCreated: (connection: Connection) => void;
};

export default function ServiceConnectionsPanel({
  service,
  services,
  connections,
  onConnectionCreated,
}: ServiceConnectionsPanelProps) {
  const [modalOpen, setModalOpen] = useState(false);
  const [method, setMethod] = useState<ConnectionMethod>("Messaging");
  const [selectedBroker, setSelectedBroker] = useState("RabbitMQ");
  const [targetServiceId, setTargetServiceId] = useState<number | "">("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const targetServices = services.filter(
    (candidate) => candidate.id !== service.id
  );
  const serviceConnections = connections.filter(
    (connection) =>
      connection.sourceId === service.id || connection.targetId === service.id
  );

  function getServiceName(id: number) {
    return services.find((candidate) => candidate.id === id)?.name ?? "Unknown";
  }

  function openModal() {
    setMethod("Messaging");
    setSelectedBroker("RabbitMQ");
    setTargetServiceId("");
    setError("");
    setBusy(false);
    setModalOpen(true);
  }

  async function createConnection() {
    if (targetServiceId === "") {
      setError("Please select a destination microservice.");
      return;
    }

    if (service.id === targetServiceId) {
      setError("A service cannot connect to itself.");
      return;
    }

    const targetService = services.find(
      (candidate) => candidate.id === targetServiceId
    );
    if (!targetService) {
      setError("Could not find the selected microservice.");
      return;
    }

    const brokerName = method === "Messaging" ? selectedBroker : null;
    const alreadyExists = connections.some(
      (connection) =>
        connection.sourceId === service.id &&
        connection.targetId === targetServiceId &&
        connection.method === method &&
        connection.broker === brokerName
    );
    if (alreadyExists) {
      setError("This connection already exists.");
      return;
    }

    const newConnection: Connection = {
      id:
        connections.reduce(
          (highestId, connection) => Math.max(highestId, connection.id),
          0
        ) + 1,
      sourceId: service.id,
      targetId: targetServiceId,
      method,
      broker: brokerName,
    };

    if (method === "Messaging" || method === "gRPC") {
      const broker = brokers.find((candidate) => candidate.name === selectedBroker);
      if (method === "Messaging" && !broker) {
        setError("Please select a supported message broker.");
        return;
      }

      setBusy(true);
      try {
        if (method === "Messaging" && broker) {
          await invoke("configure_message_connection", {
            connectionId: newConnection.id,
            sourceService: service.name,
            targetService: targetService.name,
            broker: broker.id,
          });
        } else if (method === "gRPC") {
          await invoke("configure_grpc_connection", {
            connectionId: newConnection.id,
            sourceService: service.name,
            targetService: targetService.name,
          });
        }
      } catch (error) {
        setError(`Could not configure ${method} connection: ${String(error)}`);
        return;
      } finally {
        setBusy(false);
      }
    }

    onConnectionCreated(newConnection);
    setModalOpen(false);
    setError("");
  }

  return (
    <>
      <div className="connection-panel">
        <div className="connection-panel-heading">
          <div>
            <h3>Service connections</h3>
            <p>Connect this microservice to other services.</p>
          </div>
          <button
            className="primary-button"
            onClick={openModal}
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
              Connect {service.name} to another microservice using messaging,
              REST, gRPC or webhooks.
            </p>
            {services.length < 2 && (
              <small>Create at least two microservices to connect them.</small>
            )}
          </div>
        ) : (
          <div className="connection-list">
            {serviceConnections.map((connection) => {
              const isOutgoing = connection.sourceId === service.id;

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
                      {connection.broker ? ` · ${connection.broker}` : ""}
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

      {modalOpen && (
        <div
          className="modal-overlay"
          onClick={() => {
            if (!busy) setModalOpen(false);
          }}
        >
          <div
            className="modal connection-modal"
            onClick={(event) => event.stopPropagation()}
          >
            <div className="modal-header">
              <div>
                <div className="eyebrow">SERVICE CONNECTION</div>
                <h2>Connect microservice</h2>
                <p>Configure communication for {service.name}.</p>
              </div>
              <button
                className="icon-button"
                onClick={() => setModalOpen(false)}
                disabled={busy}
              >
                <X size={19} />
              </button>
            </div>

            <label className="field-label">1. COMMUNICATION METHOD</label>

            <div className="connection-method-grid">
              {methods.map((item) => {
                const Icon = item.icon;
                return (
                  <button
                    key={item.name}
                    className={`connection-method-option ${
                      method === item.name ? "selected" : ""
                    }`}
                    onClick={() => {
                      setMethod(item.name);
                      setError("");
                    }}
                    disabled={busy}
                  >
                    <Icon size={21} />
                    <strong>{item.name}</strong>
                    <small>{item.description}</small>
                  </button>
                );
              })}
            </div>

            {method === "Messaging" && (
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
                      onClick={() => {
                        setSelectedBroker(broker.name);
                        setError("");
                      }}
                      disabled={busy}
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
              {method === "Messaging" ? "3." : "2."} DESTINATION MICROSERVICE
            </label>

            <select
              className="text-input connection-select"
              value={targetServiceId}
              disabled={busy}
              onChange={(event) => {
                setTargetServiceId(
                  event.target.value === "" ? "" : Number(event.target.value)
                );
                setError("");
              }}
            >
              <option value="">Select a microservice...</option>
              {targetServices.map((target) => (
                <option key={target.id} value={target.id}>
                  {target.name} — {target.language}
                </option>
              ))}
            </select>

            <div className="connection-preview">
              <div className="connection-preview-service">
                <Server size={18} />
                <span>{service.name}</span>
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

            {error && <div className="connection-error">{error}</div>}

            <div className="connection-note">
              <MessageSquare size={16} />
              <span>
                {method === "Messaging"
                  ? `Messaging provisions the shared ${selectedBroker} broker and injects its URL into both services.${selectedBroker === "RabbitMQ" ? " RabbitMQ adds client packages to both service manifests." : ""} Run all installs declared packages in the Docker images.`
                  : method === "gRPC"
                    ? "gRPC adds runtime packages to both service manifests, joins them to the shared Docker network, and configures the destination at port 50051. Run all installs the packages; add your RPC handlers because Micronom does not generate application code."
                    : `${method} is currently saved as connection metadata only.`}
              </span>
            </div>

            <div className="modal-footer">
              <button
                className="secondary-button"
                onClick={() => setModalOpen(false)}
                disabled={busy}
              >
                Cancel
              </button>
              <button
                className="primary-button"
                onClick={createConnection}
                disabled={targetServices.length === 0 || busy}
              >
                {busy ? (
                  <LoaderCircle size={16} className="spin" />
                ) : (
                  <Network size={17} />
                )}
                {busy ? "Connecting..." : "Create connection"}
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
