import { Plus, X } from "lucide-react";
import type { LanguageOption } from "../types";

type ServiceCreationModalProps = {
  open: boolean;
  serviceName: string;
  language: string;
  languages: LanguageOption[];
  onServiceNameChange: (value: string) => void;
  onLanguageChange: (value: string) => void;
  onCreate: () => void | Promise<void>;
  onClose: () => void;
};

export default function ServiceCreationModal({
  open,
  serviceName,
  language,
  languages,
  onServiceNameChange,
  onLanguageChange,
  onCreate,
  onClose,
}: ServiceCreationModalProps) {
  if (!open) return null;

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={(event) => event.stopPropagation()}>
        <div className="modal-header">
          <div>
            <h2>Create microservice</h2>
            <p>Configure your new service.</p>
          </div>
          <button className="icon-button" onClick={onClose}>
            <X size={19} />
          </button>
        </div>

        <label className="field-label">SERVICE NAME</label>
        <input
          className="text-input"
          placeholder="e.g. user-service"
          value={serviceName}
          onChange={(event) => onServiceNameChange(event.target.value)}
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
              onClick={() => onLanguageChange(item.name)}
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
          <button className="secondary-button" onClick={onClose}>
            Cancel
          </button>
          <button
            className="primary-button"
            onClick={onCreate}
            disabled={!serviceName.trim()}
          >
            <Plus size={17} />
            Create service
          </button>
        </div>
      </div>
    </div>
  );
}
