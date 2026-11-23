import {
  Globe,
  Network,
  RadioTower,
  Webhook,
} from "lucide-react";
import type {
  ConnectionMethod,
  DatabaseType,
  LanguageOption,
  MessageBrokerType,
} from "./types";

export const databaseOptions: {
  id: DatabaseType;
  name: string;
  description: string;
}[] = [
  {
    id: "mongo",
    name: "MongoDB",
    description: "Document database, configured in a separate container.",
  },
  {
    id: "postgres",
    name: "PostgreSQL",
    description: "Relational database with persistent storage.",
  },
  {
    id: "redis",
    name: "Redis",
    description: "In-memory data store with persistent volume.",
  },
  {
    id: "faiss",
    name: "FAISS",
    description: "Embedded vector index for Python services.",
  },
];

export const languages: LanguageOption[] = [
  { name: "Node.js", framework: "Express", icon: "JS" },
  { name: "Python", framework: "FastAPI", icon: "PY" },
  { name: "Go", framework: "Gin", icon: "GO" },
  { name: "Rust", framework: "Axum", icon: "RS" },
];

export const methods: {
  name: ConnectionMethod;
  description: string;
  icon: typeof Network;
}[] = [
  {
    name: "Messaging",
    description: "Shared brokers, URLs, and RabbitMQ client dependencies.",
    icon: RadioTower,
  },
  {
    name: "REST API",
    description: "HTTP requests between microservices.",
    icon: Globe,
  },
  {
    name: "gRPC",
    description: "gRPC packages and shared-network address configuration.",
    icon: Network,
  },
  {
    name: "Webhooks",
    description: "Send event notifications between services.",
    icon: Webhook,
  },
];

export const brokers: {
  id: MessageBrokerType;
  name: string;
  description: string;
  icon: string;
}[] = [
  {
    id: "rabbit_mq",
    name: "RabbitMQ",
    description: "Reliable message queues and routing.",
    icon: "RMQ",
  },
  {
    id: "kafka",
    name: "Apache Kafka",
    description: "Distributed event streaming.",
    icon: "KFK",
  },
  {
    id: "redis_streams",
    name: "Redis Streams",
    description: "Lightweight stream-based messaging.",
    icon: "RED",
  },
];
