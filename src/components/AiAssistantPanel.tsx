import { LoaderCircle, Play, Send } from "lucide-react";
import tzukGnome from "../assets/tzuk-gnome.png";
import type {
  AiSuggestion,
  CommandExecution,
} from "../types";

type AiAssistantPanelProps = {
  model: string;
  prompt: string;
  suggestion: AiSuggestion | null;
  execution: CommandExecution | null;
  error: string;
  busy: "suggesting" | "running" | null;
  onModelChange: (value: string) => void;
  onPromptChange: (value: string) => void;
  onAsk: () => void;
  onRun: () => void;
};

export default function AiAssistantPanel({
  model,
  prompt,
  suggestion,
  execution,
  error,
  busy,
  onModelChange,
  onPromptChange,
  onAsk,
  onRun,
}: AiAssistantPanelProps) {
  return (
    <div className="ai-panel">
      <div className="ai-panel-heading">
        <div className="ai-panel-icon">
          <img src={tzukGnome} alt="" />
        </div>
        <div>
          <h3>Tzuk</h3>
          <p>Local development assistant powered by Ollama.</p>
        </div>
      </div>

      <label className="ai-model-label" htmlFor="ollama-model">
        OLLAMA MODEL
      </label>
      <input
        id="ollama-model"
        className="ai-input"
        value={model}
        onChange={(event) => onModelChange(event.target.value)}
        placeholder="e.g. qwen2.5-coder:7b"
        disabled={busy !== null}
      />

      <label className="ai-model-label" htmlFor="ai-request">
        WHAT SHOULD I DO?
      </label>
      <textarea
        id="ai-request"
        className="ai-input ai-prompt"
        value={prompt}
        onChange={(event) => onPromptChange(event.target.value)}
        placeholder="e.g. Install this service's dependencies"
        maxLength={4000}
        disabled={busy !== null}
      />

      <div className="ai-actions">
        <button
          className="primary-button"
          onClick={onAsk}
          disabled={busy !== null || !prompt.trim() || !model.trim()}
        >
          {busy === "suggesting" ? (
            <LoaderCircle size={16} className="spin" />
          ) : (
            <Send size={16} />
          )}
          {busy === "suggesting" ? "Asking Ollama..." : "Ask AI"}
        </button>
      </div>

      {busy === "suggesting" && (
        <p className="ai-status" role="status" aria-live="polite">
          Waiting for local Ollama. The first response may take a little
          longer.
        </p>
      )}

      <p className="ai-safety-note">
        Commands are limited to predefined language-specific tasks. Review each
        suggestion and confirm before it runs.
      </p>

      {suggestion && (
        <div className="ai-suggestion">
          <strong>
            {suggestion.command ? "Suggestion" : "No command to run"}
          </strong>
          <p>{suggestion.explanation}</p>
          {suggestion.command ? (
            <>
              <code>{suggestion.command}</code>
              <button
                className="primary-button"
                onClick={onRun}
                disabled={busy !== null}
              >
                {busy === "running" ? (
                  <LoaderCircle size={16} className="spin" />
                ) : (
                  <Play size={16} />
                )}
                Run this command
              </button>
            </>
          ) : (
            <p>
              This assistant only recommends predefined development commands.
              Try a request such as “Install dependencies.”
            </p>
          )}
        </div>
      )}

      {execution && (
        <div className={`ai-output ${execution.success ? "success" : "failure"}`}>
          <strong>
            {execution.success ? "Command completed" : "Command failed"}:{" "}
            {execution.command}
          </strong>
          <pre>{execution.output}</pre>
        </div>
      )}

      {error && <p className="ai-error" role="alert">{error}</p>}
    </div>
  );
}
