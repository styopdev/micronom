export type Service = {
  id: number;
  name: string;
  language: string;
  framework: string;
  status: "Created";
  database?: DatabaseType | null;
};

export type DatabaseType = "mongo" | "postgres" | "redis" | "faiss";
export type ConnectionMethod = "Messaging" | "REST API" | "gRPC" | "Webhooks";
export type MessageBrokerType = "rabbit_mq" | "kafka" | "redis_streams";

export type Connection = {
  id: number;
  sourceId: number;
  targetId: number;
  method: ConnectionMethod;
  broker: string | null;
};

export type AiSuggestion = {
  commandId: string | null;
  command: string | null;
  explanation: string;
};

export type CommandExecution = {
  command: string;
  success: boolean;
  output: string;
};

export type ServiceActionProgress = {
  name: string;
  message: string;
};

export type LanguageOption = {
  name: string;
  framework: string;
  icon: string;
};
