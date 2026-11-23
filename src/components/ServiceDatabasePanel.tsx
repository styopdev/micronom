import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Database, Plus, X, LoaderCircle } from "lucide-react";
import { databaseOptions } from "../data";
import type { DatabaseType, Service } from "../types";

type ServiceDatabasePanelProps = {
  service: Service;
  onDatabaseConfigured: (serviceId: number, database: DatabaseType) => void;
};

export default function ServiceDatabasePanel({
  service,
  onDatabaseConfigured,
}: ServiceDatabasePanelProps) {
  const [modalOpen, setModalOpen] = useState(false);
  const [selection, setSelection] = useState<DatabaseType>("postgres");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  async function configureDatabase() {
    setBusy(true);
    setError("");

    try {
      const database = await invoke<DatabaseType>("configure_service_database", {
        serviceName: service.name,
        databaseType: selection,
      });
      onDatabaseConfigured(service.id, database);
      setModalOpen(false);
    } catch (error) {
      setError(`Could not configure database: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <div className="database-panel">
        <div className="database-panel-heading">
          <div>
            <h3>Database</h3>
            <p>Provision one persistent database for this microservice.</p>
          </div>
          {!service.database && (
            <button
              className="primary-button"
              onClick={() => {
                setSelection("postgres");
                setError("");
                setModalOpen(true);
              }}
            >
              <Plus size={17} />
              Connect database
            </button>
          )}
        </div>

        {service.database ? (
          <div className="database-configured">
            <Database size={18} />
            <div>
              <strong>
                {
                  databaseOptions.find(
                    (database) => database.id === service.database
                  )?.name
                }{" "}
                configured
              </strong>
              <p>
                {service.database === "faiss"
                  ? "A FAISS index volume will be mounted in this Python service."
                  : "A persistent database container and connection URL are configured."}{" "}
                Click Run all to start the service with its database.
              </p>
            </div>
          </div>
        ) : (
          <p className="database-empty">
            Select MongoDB, PostgreSQL, Redis, or FAISS for Python services.
            Credentials are stored in the service’s ignored .env file.
          </p>
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
            className="modal database-modal"
            onClick={(event) => event.stopPropagation()}
          >
            <div className="modal-header">
              <div>
                <div className="eyebrow">DATABASE SETUP</div>
                <h2>Connect a database</h2>
                <p>Choose one database for {service.name}.</p>
              </div>
              <button
                className="icon-button"
                onClick={() => setModalOpen(false)}
                title="Close database setup"
                disabled={busy}
              >
                <X size={19} />
              </button>
            </div>

            <div className="database-option-list">
              {databaseOptions.map((database) => {
                const unavailable =
                  database.id === "faiss" && service.language !== "Python";

                return (
                  <button
                    key={database.id}
                    className={`database-option ${
                      selection === database.id ? "selected" : ""
                    }`}
                    onClick={() => {
                      setSelection(database.id);
                      setError("");
                    }}
                    disabled={unavailable || busy}
                    title={
                      unavailable
                        ? "FAISS is currently supported only for Python services"
                        : undefined
                    }
                  >
                    <Database size={20} />
                    <span>
                      <strong>{database.name}</strong>
                      <small>
                        {unavailable
                          ? "Currently available for Python services only."
                          : database.description}
                      </small>
                    </span>
                    <span className="language-radio">
                      {selection === database.id ? "✓" : ""}
                    </span>
                  </button>
                );
              })}
            </div>

            <div className="connection-note">
              <Database size={16} />
              <span>
                MongoDB, PostgreSQL, and Redis get a persistent Docker volume.
                FAISS is embedded in the Python service and uses a persistent
                index volume. This provisions the database and connection
                settings; it does not generate query or search code. Run all
                starts the configured database with the service.
              </span>
            </div>

            {error && (
              <div className="connection-error" role="alert">
                {error}
              </div>
            )}

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
                onClick={configureDatabase}
                disabled={busy}
              >
                {busy ? (
                  <LoaderCircle size={16} className="spin" />
                ) : (
                  <Database size={17} />
                )}
                {busy ? "Configuring..." : "Add database"}
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
