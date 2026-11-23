
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tauri::Emitter;

const COMMAND_TIMEOUT: Duration = Duration::from_secs(120);
const DOCKER_COMMAND_TIMEOUT: Duration = Duration::from_secs(600);
const MAX_COMMAND_OUTPUT: usize = 16 * 1024;

fn write_file(
    service_dir: &Path,
    relative_path: &str,
    contents: &str,
) -> Result<(), String> {
    let path = service_dir.join(relative_path);

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Could not create directory: {e}"))?;
    }

    std::fs::write(&path, contents)
        .map_err(|e| format!("Could not write {}: {e}", relative_path))
}

fn find_available_port() -> Result<u16, String> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|e| format!("Could not allocate a port: {e}"))?;

    let port = listener.local_addr()
        .map_err(|e| format!("Could not read allocated port: {e}"))?
        .port();

    drop(listener);
    Ok(port)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
enum DatabaseType {
    Mongo,
    Postgres,
    Redis,
    Faiss,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum BrokerType {
    RabbitMq,
    Kafka,
    RedisStreams,
}

impl BrokerType {
    fn as_str(self) -> &'static str {
        match self {
            Self::RabbitMq => "rabbit_mq",
            Self::Kafka => "kafka",
            Self::RedisStreams => "redis_streams",
        }
    }

    fn env_key(self) -> &'static str {
        match self {
            Self::RabbitMq => "RABBITMQ_URL",
            Self::Kafka => "KAFKA_BOOTSTRAP_SERVERS",
            Self::RedisStreams => "REDIS_STREAMS_URL",
        }
    }

    fn secret_key(self) -> Option<&'static str> {
        match self {
            Self::RabbitMq => Some("RABBITMQ_PASSWORD"),
            Self::Kafka => None,
            Self::RedisStreams => Some("REDIS_STREAMS_PASSWORD"),
        }
    }

    fn connection_url(self, password: Option<&str>) -> Result<String, String> {
        match self {
            Self::RabbitMq => {
                let password = password
                    .ok_or_else(|| "RabbitMQ credentials were not generated.".to_string())?;
                Ok(format!("amqp://micronom:{password}@rabbitmq:5672"))
            }
            Self::Kafka => Ok("kafka:9092".into()),
            Self::RedisStreams => {
                let password = password
                    .ok_or_else(|| "Redis Streams credentials were not generated.".to_string())?;
                Ok(format!("redis://:{password}@redis-streams:6379/0"))
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum ServiceConnectionType {
    #[default]
    Messaging,
    Grpc,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ServiceConnection {
    id: u64,
    source_service: String,
    target_service: String,
    #[serde(default)]
    method: ServiceConnectionType,
    #[serde(default)]
    broker: Option<BrokerType>,
}

impl DatabaseType {
    fn label(self) -> &'static str {
        match self {
            Self::Mongo => "MongoDB",
            Self::Postgres => "PostgreSQL",
            Self::Redis => "Redis",
            Self::Faiss => "FAISS",
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Mongo => "mongo",
            Self::Postgres => "postgres",
            Self::Redis => "redis",
            Self::Faiss => "faiss",
        }
    }
}

fn generate_database_password() -> Result<String, String> {
    let mut random_bytes = [0u8; 32];
    getrandom::fill(&mut random_bytes)
        .map_err(|error| format!("Could not generate database credentials: {error}"))?;

    Ok(random_bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn database_env_entries(
    service_name: &str,
    database_type: DatabaseType,
    password: Option<&str>,
) -> Result<Vec<(&'static str, String)>, String> {
    let mut entries = vec![(
        "DATABASE_TYPE",
        database_type.as_str().to_string(),
    )];

    match database_type {
        DatabaseType::Mongo => {
            let password = password
                .ok_or_else(|| "MongoDB password was not generated.".to_string())?;
            entries.extend([
                ("MONGO_USER", "micronom".into()),
                ("MONGO_PASSWORD", password.into()),
                (
                    "DATABASE_URL",
                    format!(
                        "mongodb://micronom:{password}@db-{service_name}:27017/{service_name}?authSource=admin"
                    ),
                ),
            ]);
        }
        DatabaseType::Postgres => {
            let password = password
                .ok_or_else(|| "PostgreSQL password was not generated.".to_string())?;
            entries.extend([
                ("POSTGRES_PASSWORD", password.into()),
                (
                    "DATABASE_URL",
                    format!(
                        "postgresql://micronom:{password}@db-{service_name}:5432/{service_name}"
                    ),
                ),
            ]);
        }
        DatabaseType::Redis => {
            let password = password
                .ok_or_else(|| "Redis password was not generated.".to_string())?;
            entries.extend([
                ("REDIS_PASSWORD", password.into()),
                (
                    "DATABASE_URL",
                    format!("redis://:{password}@db-{service_name}:6379/0"),
                ),
            ]);
        }
        DatabaseType::Faiss => {
            entries.push(("FAISS_INDEX_PATH", "/data/faiss".into()));
        }
    }

    Ok(entries)
}

fn generate_database_compose(service_name: &str, database_type: DatabaseType) -> String {
    let database_service_name = format!("db-{service_name}");
    match database_type {
        DatabaseType::Mongo => format!(
            r#"services:
  {service_name}:
    environment:
      DATABASE_TYPE: mongo
      DATABASE_URL: "${{DATABASE_URL}}"
    depends_on:
      {database_service_name}:
        condition: service_healthy
  {database_service_name}:
    image: mongo:8
    environment:
      MONGO_INITDB_ROOT_USERNAME: "${{MONGO_USER}}"
      MONGO_INITDB_ROOT_PASSWORD: "${{MONGO_PASSWORD}}"
      MONGO_INITDB_DATABASE: "{service_name}"
    volumes:
      - mongo-data:/data/db
      - ./.micronom/mongo-init.js:/docker-entrypoint-initdb.d/01-micronom-init.js:ro
    healthcheck:
      test: ["CMD", "mongosh", "--quiet", "--eval", "db.adminCommand('ping')"]
      interval: 5s
      timeout: 5s
      retries: 10
volumes:
  mongo-data: {{}}
"#
        ),
        DatabaseType::Postgres => format!(
            r#"services:
  {service_name}:
    environment:
      DATABASE_TYPE: postgres
      DATABASE_URL: "${{DATABASE_URL}}"
    depends_on:
      {database_service_name}:
        condition: service_healthy
  {database_service_name}:
    image: postgres:17-alpine
    environment:
      POSTGRES_DB: "{service_name}"
      POSTGRES_USER: micronom
      POSTGRES_PASSWORD: "${{POSTGRES_PASSWORD}}"
    volumes:
      - postgres-data:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U micronom -d {service_name}"]
      interval: 5s
      timeout: 5s
      retries: 10
volumes:
  postgres-data: {{}}
"#
        ),
        DatabaseType::Redis => format!(
            r#"services:
  {service_name}:
    environment:
      DATABASE_TYPE: redis
      DATABASE_URL: "${{DATABASE_URL}}"
    depends_on:
      {database_service_name}:
        condition: service_healthy
  {database_service_name}:
    image: redis:7-alpine
    environment:
      REDIS_PASSWORD: "${{REDIS_PASSWORD}}"
      REDISCLI_AUTH: "${{REDIS_PASSWORD}}"
    command: ["redis-server", "--appendonly", "yes", "--requirepass", "${{REDIS_PASSWORD}}"]
    volumes:
      - redis-data:/data
    healthcheck:
      test: ["CMD", "redis-cli", "ping"]
      interval: 5s
      timeout: 5s
      retries: 10
volumes:
  redis-data: {{}}
"#
        ),
        DatabaseType::Faiss => format!(
            r#"services:
  {service_name}:
    environment:
      DATABASE_TYPE: faiss
      FAISS_INDEX_PATH: /data/faiss
    volumes:
      - faiss-index:/data/faiss
volumes:
  faiss-index: {{}}
"#
        ),
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ExistingService {
    name: String,
    language: String,
    framework: String,
    database: Option<DatabaseType>,
}

fn detect_service_language(directory: &Path) -> Option<(&'static str, &'static str)> {
    if directory.join("package.json").is_file() {
        Some(("Node.js", "Express"))
    } else if directory.join("requirements.txt").is_file() {
        Some(("Python", "FastAPI"))
    } else if directory.join("go.mod").is_file() {
        Some(("Go", "Gin"))
    } else if directory.join("Cargo.toml").is_file() {
        Some(("Rust", "Axum"))
    } else {
        None
    }
}

fn read_database_type(directory: &Path) -> Result<Option<DatabaseType>, String> {
    let path = directory.join(".micronom").join("database.json");
    match std::fs::read_to_string(path) {
        Ok(contents) => serde_json::from_str(&contents)
            .map(Some)
            .map_err(|error| format!("Could not read service database configuration: {error}")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("Could not read service database configuration: {error}")),
    }
}

fn service_connections_path(app_dir: &Path) -> PathBuf {
    app_dir.join(".micronom").join("connections.json")
}

fn read_service_connections(app_dir: &Path) -> Result<Vec<ServiceConnection>, String> {
    match std::fs::read_to_string(service_connections_path(app_dir)) {
        Ok(contents) => serde_json::from_str(&contents)
            .map_err(|error| format!("Could not read service connections: {error}")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(format!("Could not read service connections: {error}")),
    }
}

fn write_service_connections(
    app_dir: &Path,
    connections: &[ServiceConnection],
) -> Result<(), String> {
    let contents = serde_json::to_string_pretty(connections)
        .map_err(|error| format!("Could not save service connections: {error}"))?;
    write_file(
        app_dir,
        ".micronom/connections.json",
        &format!("{contents}\n"),
    )
}

#[tauri::command]
fn list_service_connections() -> Result<Vec<ServiceConnection>, String> {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .ok_or_else(|| "Could not find the user home directory.".to_string())?;
    read_service_connections(&PathBuf::from(home).join("app"))
}

#[tauri::command]
fn list_service_folders() -> Result<Vec<ExistingService>, String> {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .ok_or_else(|| "Could not find the user home directory.".to_string())?;
    let app_dir = PathBuf::from(home).join("app");

    if !app_dir.exists() {
        return Ok(Vec::new());
    }

    let entries = std::fs::read_dir(&app_dir)
        .map_err(|error| format!("Could not read the Micronom app directory: {error}"))?;
    let mut services = Vec::new();

    for entry in entries {
        let entry = entry.map_err(|error| format!("Could not read a service entry: {error}"))?;
        if !entry
            .file_type()
            .map_err(|error| format!("Could not inspect a service entry: {error}"))?
            .is_dir()
        {
            continue;
        }

        let name = entry.file_name().to_string_lossy().into_owned();
        if name.is_empty()
            || !name.chars().all(|character| {
                character.is_ascii_lowercase()
                    || character.is_ascii_digit()
                    || character == '-'
            })
        {
            continue;
        }

        let directory = entry.path();
        let metadata = detect_service_language(&directory);

        if let Some((language, framework)) = metadata {
            services.push(ExistingService {
                name,
                language: language.into(),
                framework: framework.into(),
                database: read_database_type(&directory)?,
            });
        }
    }

    services.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(services)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct CommandSuggestion {
    command_id: Option<String>,
    command: Option<String>,
    explanation: String,
}

#[derive(serde::Deserialize)]
struct OllamaResponse {
    message: OllamaMessage,
}

#[derive(serde::Deserialize)]
struct OllamaMessage {
    content: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelSuggestion {
    command_id: Option<String>,
    explanation: String,
}

fn command_for_id(
    command_id: &str,
) -> Option<(&'static str, &'static [&'static str], &'static str)> {
    match command_id {
        "node-install" => Some(("npm", &["install"], "npm install")),
        "python-install" => Some((
            "python",
            &["-m", "pip", "install", "-r", "requirements.txt"],
            "python -m pip install -r requirements.txt",
        )),
        "python-check" => Some((
            "python",
            &["-m", "compileall", "app"],
            "python -m compileall app",
        )),
        "go-download" => Some(("go", &["mod", "download"], "go mod download")),
        "go-test" => Some(("go", &["test", "./..."], "go test ./...")),
        "rust-check" => Some(("cargo", &["check"], "cargo check")),
        "rust-test" => Some(("cargo", &["test"], "cargo test")),
        _ => None,
    }
}

fn allowed_command_ids(language: &str) -> Option<&'static [&'static str]> {
    match language {
        "Node.js" => Some(&["node-install"]),
        "Python" => Some(&["python-install", "python-check"]),
        "Go" => Some(&["go-download", "go-test"]),
        "Rust" => Some(&["rust-check", "rust-test"]),
        _ => None,
    }
}

#[cfg(test)]
mod command_tests {
    use super::{
        BrokerType, DatabaseType, ServiceConnection, ServiceConnectionType,
        allowed_command_ids, command_for_id, database_env_entries, ensure_service_dependencies,
        generate_database_compose, generate_service_connection_compose,
        generate_workspace_broker_compose, sync_service_connection_artifacts,
    };
    use std::path::PathBuf;
    use std::process::Command;

    fn temporary_app_directory() -> PathBuf {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "micronom-messaging-test-{}-{timestamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn create_test_service(app_dir: &std::path::Path, name: &str) {
        let service_dir = app_dir.join(name);
        std::fs::create_dir_all(&service_dir).unwrap();
        std::fs::write(service_dir.join(".env"), "EXISTING=value\n").unwrap();
        std::fs::write(
            service_dir.join("docker-compose.yml"),
            format!("services:\n  {name}:\n    image: alpine\n"),
        )
        .unwrap();
        std::fs::write(service_dir.join(".dockerignore"), "").unwrap();
        std::fs::write(
            service_dir.join("package.json"),
            r#"{"name":"test-service","dependencies":{"express":"^5.1.0"}}"#,
        )
        .unwrap();
        std::fs::write(service_dir.join("README.md"), "# Test service\n").unwrap();
    }

    fn validate_compose_config(directory: &std::path::Path, files: &[&str]) {
        let mut command = Command::new("docker");
        command.arg("compose");
        for file in files {
            command.args(["-f", file]);
        }
        let output = command
            .arg("config")
            .current_dir(directory)
            .output();

        match output {
            Ok(output) => assert!(
                output.status.success(),
                "Docker Compose config failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                eprintln!("Skipping Docker Compose config validation: Docker is not installed.");
            }
            Err(error) => panic!("Could not execute Docker Compose config: {error}"),
        }
    }

    #[test]
    fn command_allowlist_is_scoped_to_service_language() {
        assert!(allowed_command_ids("Go").unwrap().contains(&"go-test"));
        assert!(!allowed_command_ids("Go").unwrap().contains(&"rust-test"));
    }

    #[test]
    fn unknown_command_ids_are_rejected() {
        assert!(command_for_id("powershell").is_none());
    }

    #[test]
    fn postgres_configuration_has_a_persistent_healthy_service() {
        let compose = generate_database_compose("catalog", DatabaseType::Postgres);
        let environment =
            database_env_entries("catalog", DatabaseType::Postgres, Some("secret")).unwrap();

        assert!(compose.contains("image: postgres:17-alpine"));
        assert!(compose.contains("postgres-data:/var/lib/postgresql/data"));
        assert!(compose.contains("condition: service_healthy"));
        assert!(environment.iter().any(|(key, value)| {
            *key == "DATABASE_URL"
                && value == "postgresql://micronom:secret@db-catalog:5432/catalog"
        }));
    }

    #[test]
    fn mongo_and_redis_store_passwords_outside_compose_files() {
        let mongo_compose = generate_database_compose("catalog", DatabaseType::Mongo);
        let redis_compose = generate_database_compose("catalog", DatabaseType::Redis);
        let mongo_environment =
            database_env_entries("catalog", DatabaseType::Mongo, Some("secret")).unwrap();
        let redis_environment =
            database_env_entries("catalog", DatabaseType::Redis, Some("secret")).unwrap();

        assert!(mongo_compose.contains("mongo-data:/data/db"));
        assert!(mongo_compose.contains("mongo-init.js:/docker-entrypoint-initdb.d"));
        assert!(redis_compose.contains("redis-data:/data"));
        assert!(!mongo_compose.contains("secret"));
        assert!(!redis_compose.contains("secret"));
        assert!(mongo_environment.iter().any(|(key, value)| {
            *key == "DATABASE_URL"
                && value == "mongodb://micronom:secret@db-catalog:27017/catalog?authSource=admin"
        }));
        assert!(redis_environment.iter().any(|(key, value)| {
            *key == "DATABASE_URL" && value == "redis://:secret@db-catalog:6379/0"
        }));
    }

    #[test]
    fn faiss_uses_a_persistent_python_index_volume() {
        let compose = generate_database_compose("catalog", DatabaseType::Faiss);
        let environment = database_env_entries("catalog", DatabaseType::Faiss, None).unwrap();

        assert!(compose.contains("FAISS_INDEX_PATH: /data/faiss"));
        assert!(compose.contains("faiss-index:/data/faiss"));
        assert!(environment.iter().any(|(key, value)| {
            *key == "FAISS_INDEX_PATH" && value == "/data/faiss"
        }));
    }

    #[test]
    fn broker_compose_contains_only_the_selected_persistent_services() {
        let compose = generate_workspace_broker_compose(&[
            BrokerType::RabbitMq,
            BrokerType::Kafka,
            BrokerType::RedisStreams,
        ]);

        assert!(compose.contains("image: rabbitmq:4-management"));
        assert!(compose.contains("image: apache/kafka:4.1.1"));
        assert!(compose.contains("image: redis:7-alpine"));
        assert!(compose.contains("rabbitmq-data:/var/lib/rabbitmq"));
        assert!(compose.contains("kafka-data:/var/lib/kafka/data"));
        assert!(compose.contains("redis-streams-data:/data"));
        assert!(compose.contains("external: true"));

        let kafka_only = generate_workspace_broker_compose(&[BrokerType::Kafka]);
        assert!(kafka_only.contains("image: apache/kafka:4.1.1"));
        assert!(!kafka_only.contains("image: rabbitmq:4-management"));
        assert!(!kafka_only.contains("image: redis:7-alpine"));
    }

    #[test]
    fn service_connection_overlay_exports_the_selected_broker_urls() {
        let environment_keys = vec![
            "RABBITMQ_URL".to_string(),
            "KAFKA_BOOTSTRAP_SERVERS".to_string(),
        ];
        let compose = generate_service_connection_compose(
            "catalog",
            &environment_keys,
        );

        assert!(compose.contains("RABBITMQ_URL: \"${RABBITMQ_URL}\""));
        assert!(compose.contains("KAFKA_BOOTSTRAP_SERVERS: \"${KAFKA_BOOTSTRAP_SERVERS}\""));
        assert!(!compose.contains("REDIS_STREAMS_URL"));
        assert!(compose.contains("name: micronom-services"));
    }

    #[test]
    fn syncing_message_connections_writes_credentials_urls_and_service_overlays() {
        let app_dir = temporary_app_directory();
        create_test_service(&app_dir, "catalog");
        create_test_service(&app_dir, "worker");
        let connections = vec![
            ServiceConnection {
                id: 1,
                source_service: "catalog".into(),
                target_service: "worker".into(),
                method: ServiceConnectionType::Messaging,
                broker: Some(BrokerType::RabbitMq),
            },
            ServiceConnection {
                id: 2,
                source_service: "catalog".into(),
                target_service: "worker".into(),
                method: ServiceConnectionType::Messaging,
                broker: Some(BrokerType::Kafka),
            },
            ServiceConnection {
                id: 3,
                source_service: "catalog".into(),
                target_service: "worker".into(),
                method: ServiceConnectionType::Messaging,
                broker: Some(BrokerType::RedisStreams),
            },
        ];

        sync_service_connection_artifacts(&app_dir, &connections).unwrap();

        let secrets =
            std::fs::read_to_string(app_dir.join(".micronom").join("brokers").join(".env"))
                .unwrap();
        let broker_compose = std::fs::read_to_string(
            app_dir
                .join(".micronom")
                .join("brokers")
                .join("docker-compose.yml"),
        )
        .unwrap();
        let service_env = std::fs::read_to_string(app_dir.join("catalog").join(".env")).unwrap();
        let service_compose = std::fs::read_to_string(
            app_dir
                .join("catalog")
                .join("docker-compose.connections.yml"),
        )
        .unwrap();
        let worker_env = std::fs::read_to_string(app_dir.join("worker").join(".env")).unwrap();

        validate_compose_config(
            &app_dir.join(".micronom").join("brokers"),
            &["docker-compose.yml"],
        );
        validate_compose_config(
            &app_dir.join("catalog"),
            &["docker-compose.yml", "docker-compose.connections.yml"],
        );

        assert!(secrets.contains("RABBITMQ_PASSWORD="));
        assert!(secrets.contains("REDIS_STREAMS_PASSWORD="));
        assert!(!broker_compose.contains("RABBITMQ_PASSWORD="));
        assert!(service_env.contains("EXISTING=value"));
        assert!(service_env.contains("amqp://micronom:"));
        assert!(service_env.contains("KAFKA_BOOTSTRAP_SERVERS=kafka:9092"));
        assert!(service_env.contains("redis://:"));
        assert!(worker_env.contains("RABBITMQ_URL="));
        assert!(service_compose.contains("RABBITMQ_URL"));
        assert!(service_compose.contains("KAFKA_BOOTSTRAP_SERVERS"));
        let package_json =
            std::fs::read_to_string(app_dir.join("catalog").join("package.json")).unwrap();
        assert!(package_json.contains("\"amqplib\""));

        std::fs::remove_dir_all(app_dir).unwrap();
    }

    #[test]
    fn grpc_connection_adds_dependencies_and_shared_network_addresses() {
        let app_dir = temporary_app_directory();
        create_test_service(&app_dir, "api");
        create_test_service(&app_dir, "billing");
        let connections = vec![ServiceConnection {
            id: 1,
            source_service: "api".into(),
            target_service: "billing".into(),
            method: ServiceConnectionType::Grpc,
            broker: None,
        }];

        sync_service_connection_artifacts(&app_dir, &connections).unwrap();

        let source_env = std::fs::read_to_string(app_dir.join("api").join(".env")).unwrap();
        let target_env = std::fs::read_to_string(app_dir.join("billing").join(".env")).unwrap();
        let source_compose = std::fs::read_to_string(
            app_dir
                .join("api")
                .join("docker-compose.connections.yml"),
        )
        .unwrap();
        let source_manifest =
            std::fs::read_to_string(app_dir.join("api").join("package.json")).unwrap();
        let target_readme =
            std::fs::read_to_string(app_dir.join("billing").join("README.md")).unwrap();

        assert!(source_env.contains("GRPC_TARGET_BILLING=billing:50051"));
        assert!(target_env.contains("GRPC_LISTEN_ADDRESS=0.0.0.0:50051"));
        assert!(source_compose.contains("GRPC_TARGET_BILLING"));
        assert!(source_compose.contains("name: micronom-services"));
        assert!(source_manifest.contains("\"@grpc/grpc-js\""));
        assert!(source_manifest.contains("\"@grpc/proto-loader\""));
        assert!(target_readme.contains("RPC handler code"));

        validate_compose_config(
            &app_dir.join("api"),
            &["docker-compose.yml", "docker-compose.connections.yml"],
        );
        std::fs::remove_dir_all(app_dir).unwrap();
    }

    #[test]
    fn connection_dependencies_are_added_idempotently_for_every_language() {
        let app_dir = temporary_app_directory();
        let fixtures = [
            ("node", "package.json", r#"{"name":"node","dependencies":{}}"#),
            ("python", "requirements.txt", "fastapi>=0.115,<1.0\n"),
            ("go", "go.mod", "module example.com/go\n\ngo 1.24\n"),
            (
                "rust",
                "Cargo.toml",
                "[package]\nname = \"rust\"\nversion = \"0.1.0\"\n\n[dependencies]\n",
            ),
        ];

        for (name, manifest, contents) in fixtures {
            let service_dir = app_dir.join(name);
            std::fs::create_dir_all(&service_dir).unwrap();
            std::fs::write(service_dir.join(manifest), contents).unwrap();
            ensure_service_dependencies(&service_dir, true, true).unwrap();
            ensure_service_dependencies(&service_dir, true, true).unwrap();
        }

        let node = std::fs::read_to_string(app_dir.join("node").join("package.json")).unwrap();
        let python =
            std::fs::read_to_string(app_dir.join("python").join("requirements.txt")).unwrap();
        let go = std::fs::read_to_string(app_dir.join("go").join("go.mod")).unwrap();
        let rust = std::fs::read_to_string(app_dir.join("rust").join("Cargo.toml")).unwrap();

        assert_eq!(node.matches("\"amqplib\"").count(), 1);
        assert_eq!(node.matches("\"@grpc/grpc-js\"").count(), 1);
        assert_eq!(python.matches("pika>=").count(), 1);
        assert_eq!(python.matches("grpcio>=").count(), 1);
        assert_eq!(go.matches("github.com/rabbitmq/amqp091-go").count(), 1);
        assert_eq!(go.matches("google.golang.org/grpc").count(), 1);
        assert_eq!(rust.matches("lapin =").count(), 1);
        assert_eq!(rust.matches("tonic =").count(), 1);

        std::fs::remove_dir_all(app_dir).unwrap();
    }

    #[test]
    fn old_messaging_connection_records_default_to_messaging() {
        let old_format = r#"[{"id":1,"sourceService":"api","targetService":"billing","broker":"rabbit_mq"}]"#;
        let connections: Vec<ServiceConnection> = serde_json::from_str(old_format).unwrap();

        assert_eq!(connections[0].method, ServiceConnectionType::Messaging);
        assert_eq!(connections[0].broker, Some(BrokerType::RabbitMq));
    }
}

#[tauri::command]
async fn suggest_service_command(
    model: String,
    prompt: String,
    language: String,
    framework: String,
) -> Result<CommandSuggestion, String> {
    if model.is_empty()
        || model.len() > 100
        || !model
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.:".contains(c))
    {
        return Err("Enter a valid Ollama model name.".into());
    }

    if prompt.trim().is_empty() || prompt.len() > 4000 {
        return Err("Enter a request of up to 4,000 characters.".into());
    }

    let allowed_ids = allowed_command_ids(&language)
        .ok_or_else(|| "Unsupported service language.".to_string())?;
    let command_options = allowed_ids
        .iter()
        .filter_map(|id| command_for_id(id).map(|(_, _, command)| format!("- {id}: {command}")))
        .collect::<Vec<_>>()
        .join("\n");

    let request = serde_json::json!({
        "model": model,
        "stream": false,
        "format": "json",
        "messages": [
            {
                "role": "system",
                "content": format!(
                    "You are Micronom's local development assistant. Choose at most one command from this exact allowlist:\n{command_options}\n\
                    Return JSON with keys commandId and explanation. commandId must be one listed ID or null if none applies. \
                    Never invent or suggest shell commands, command arguments, code, or destructive actions. \
                    The selected command will be displayed to the user for confirmation before it runs."
                )
            },
            {
                "role": "user",
                "content": format!(
                    "Service language: {language} ({framework}). Request: {}",
                    prompt.trim()
                )
            }
        ]
    });

    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|error| format!("Could not create the Ollama client: {error}"))?
        .post("http://127.0.0.1:11434/api/chat")
        .json(&request)
        .send()
        .await
        .map_err(|error| {
            format!("Could not reach Ollama at 127.0.0.1:11434. Ensure Ollama is running: {error}")
        })?
        .error_for_status()
        .map_err(|error| format!("Ollama returned an error: {error}"))?
        .json::<OllamaResponse>()
        .await
        .map_err(|error| format!("Could not read the Ollama response: {error}"))?;

    let suggestion = serde_json::from_str::<ModelSuggestion>(&response.message.content)
        .map_err(|error| format!("Ollama returned an invalid command suggestion: {error}"))?;

    if suggestion
        .command_id
        .as_deref()
        .is_some_and(|id| !allowed_ids.contains(&id))
    {
        return Err("Ollama suggested a command outside the supported allowlist.".into());
    }

    let command_id = suggestion.command_id;
    let command = command_id
        .as_deref()
        .and_then(command_for_id)
        .map(|(_, _, command)| command.to_string());

    Ok(CommandSuggestion {
        command_id,
        command,
        explanation: suggestion.explanation,
    })
}

fn capture_output<R: Read + Send + 'static>(mut reader: R) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut captured = Vec::new();
        let mut buffer = [0; 4096];

        loop {
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(count) => {
                    let remaining = MAX_COMMAND_OUTPUT.saturating_sub(captured.len());
                    captured.extend_from_slice(&buffer[..count.min(remaining)]);
                }
            }
        }

        captured
    })
}

fn execute_fixed_command(
    program: &str,
    args: &[&str],
    service_dir: &Path,
) -> Result<(bool, String), String> {
    let mut command = if cfg!(windows) && program == "npm" {
        let mut cmd = Command::new("cmd");
        cmd.args(["/D", "/S", "/C", "npm install"]);
        cmd
    } else {
        let mut cmd = Command::new(program);
        cmd.args(args);
        cmd
    };

    let mut child = command
        .current_dir(service_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Could not start {program}: {error}"))?;

    let stdout = capture_output(
        child
            .stdout
            .take()
            .ok_or_else(|| "Could not capture command output.".to_string())?,
    );
    let stderr = capture_output(
        child
            .stderr
            .take()
            .ok_or_else(|| "Could not capture command errors.".to_string())?,
    );
    let started = Instant::now();

    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout.join();
                let _ = stderr.join();
                return Err(format!("Could not monitor {program}: {error}"));
            }
        }

        if started.elapsed() >= COMMAND_TIMEOUT {
            child
                .kill()
                .map_err(|error| format!("Could not stop timed-out {program}: {error}"))?;
            let _ = child.wait();
            let _ = stdout.join();
            let _ = stderr.join();
            return Err(format!("{program} exceeded the 120-second command limit."));
        }

        thread::sleep(Duration::from_millis(100));
    };

    let mut output = stdout
        .join()
        .map_err(|_| "Could not collect command output.".to_string())?;
    let error_output = stderr
        .join()
        .map_err(|_| "Could not collect command output.".to_string())?;
    output.extend_from_slice(&error_output);
    let text = String::from_utf8_lossy(&output).trim().to_string();

    Ok((
        status.success(),
        if text.is_empty() {
            if status.success() {
                "Command completed successfully.".into()
            } else {
                "Command failed without additional output.".into()
            }
        } else {
            text
        },
    ))
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct CommandExecution {
    command: String,
    success: bool,
    output: String,
}

#[tauri::command]
async fn execute_service_command(
    service_name: String,
    command_id: String,
    language: String,
) -> Result<CommandExecution, String> {
    tauri::async_runtime::spawn_blocking(move || {
        execute_service_command_blocking(service_name, command_id, language)
    })
    .await
    .map_err(|error| format!("Command task failed: {error}"))?
}

fn execute_service_command_blocking(
    service_name: String,
    command_id: String,
    language: String,
) -> Result<CommandExecution, String> {
    if service_name.is_empty()
        || !service_name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err("Invalid microservice name.".into());
    }

    let allowed_ids = allowed_command_ids(&language)
        .ok_or_else(|| "Unsupported service language.".to_string())?;
    if !allowed_ids.contains(&command_id.as_str()) {
        return Err("This command is not allowed for the selected service language.".into());
    }

    let (program, args, display_command) =
        command_for_id(&command_id).ok_or_else(|| "This command is not allowed.".to_string())?;

    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .ok_or_else(|| "Could not find the user home directory.".to_string())?;
    let app_dir = PathBuf::from(home).join("app");
    let canonical_app = app_dir
        .canonicalize()
        .map_err(|error| format!("Could not access the Micronom app directory: {error}"))?;
    let service_dir = canonical_app.join(&service_name);
    let canonical_service = service_dir
        .canonicalize()
        .map_err(|error| format!("Could not access the microservice directory: {error}"))?;

    if canonical_service.parent() != Some(canonical_app.as_path()) || !canonical_service.is_dir() {
        return Err(
            "The microservice must be a direct child of the Micronom app directory.".into(),
        );
    }

    let (success, output) = execute_fixed_command(program, args, &canonical_service)?;

    Ok(CommandExecution {
        command: display_command.into(),
        success,
        output,
    })
}

#[tauri::command]
fn create_service_folder(
    name: String,
    language: String,
    framework: String,
) -> Result<String, String> {
    // Accept safe, lowercase service names only.
    if name.is_empty()
        || name.len() > 48
        || !name.as_bytes()[0].is_ascii_alphanumeric()
        || !name.as_bytes()[name.len() - 1].is_ascii_alphanumeric()
        || !name.chars().all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'
        })
    {
        return Err(
            "Invalid service name. Use lowercase letters, numbers, and hyphens."
                .into()
        );
    }

    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .ok_or_else(|| "Could not find the user's home directory".to_string())?;

    let app_dir = PathBuf::from(home).join("app");

    std::fs::create_dir_all(&app_dir)
        .map_err(|e| format!("Could not create app directory: {e}"))?;

    let service_dir = app_dir.join(&name);

    if service_dir.exists() {
        return Err(format!("A service named '{}' already exists.", name));
    }

    let (container_port, files, local_instructions): (
        u16,
        Vec<(&str, &str)>,
        &str,
    ) = match (language.as_str(), framework.as_str()) {

        // NODE.JS + EXPRESS
        ("Node.js", "Express") => (
            3000,
            vec![
                ("src/index.js", r#"
const express = require("express");

const app = express();
const port = Number(process.env.PORT) || 3000;

app.use(express.json());

app.get("/", (_req, res) => {
  res.json({
    service: "SERVICE_NAME",
    message: "API is running"
  });
});

app.get("/health", (_req, res) => {
  res.json({ status: "ok" });
});

app.listen(port, "0.0.0.0", () => {
  console.log(`SERVICE_NAME listening on port ${port}`);
});
"#),
                ("package.json", r#"
{
  "name": "SERVICE_NAME",
  "version": "1.0.0",
  "private": true,
  "description": "Microservice generated by Micronom",
  "main": "src/index.js",
  "scripts": {
    "start": "node src/index.js",
    "dev": "node --watch src/index.js"
  },
  "dependencies": {
    "express": "^5.1.0"
  }
}
"#),
                ("Dockerfile", r#"
FROM node:22-alpine
WORKDIR /app
COPY package.json ./
RUN npm install --omit=dev
COPY . .
ENV PORT=3000
EXPOSE 3000
CMD ["npm", "start"]
"#),
            ],
            "```powershell\nnpm install\nnpm run dev\n```",
        ),

        // PYTHON + FASTAPI
        ("Python", "FastAPI") => (
            8000,
            vec![
                ("app/main.py", r#"
import os
from fastapi import FastAPI

app = FastAPI(title="SERVICE_NAME")

@app.get("/")
async def root():
    return {
        "service": "SERVICE_NAME",
        "message": "API is running"
    }

@app.get("/health")
async def health():
    return {"status": "ok"}
"#),
                ("requirements.txt", r#"
fastapi>=0.115,<1.0
uvicorn[standard]>=0.34,<1.0
"#),
                ("Dockerfile", r#"
FROM python:3.12-slim
WORKDIR /app
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt
COPY . .
ENV PORT=8000
EXPOSE 8000
CMD ["sh", "-c", "uvicorn app.main:app --host 0.0.0.0 --port ${PORT:-8000}"]
"#),
            ],
            "```powershell\npy -m venv .venv\n.venv\\Scripts\\Activate.ps1\npip install -r requirements.txt\nuvicorn app.main:app --reload --port 8000\n```",
        ),

        // GO + GIN
        ("Go", "Gin") => (
            8080,
            vec![
                ("main.go", r#"
package main

import (
    "net/http"
    "os"

    "github.com/gin-gonic/gin"
)

func main() {
    router := gin.Default()

    router.GET("/", func(c *gin.Context) {
        c.JSON(http.StatusOK, gin.H{
            "service": "SERVICE_NAME",
            "message": "API is running",
        })
    })

    router.GET("/health", func(c *gin.Context) {
        c.JSON(http.StatusOK, gin.H{
            "status": "ok",
        })
    })

    port := os.Getenv("PORT")
    if port == "" {
        port = "8080"
    }

    if err := router.Run(":" + port); err != nil {
        panic(err)
    }
}
"#),
                ("go.mod", r#"
module example.com/SERVICE_NAME

go 1.24

require github.com/gin-gonic/gin v1.10.0
"#),
                ("Dockerfile", r#"
FROM golang:1.24-alpine AS builder
WORKDIR /app
COPY go.mod ./
RUN go mod download
COPY . .
RUN CGO_ENABLED=0 GOOS=linux go build -o server .

FROM alpine:3.21
WORKDIR /app
COPY --from=builder /app/server .
ENV PORT=8080
EXPOSE 8080
CMD ["./server"]
"#),
            ],
            "```powershell\ngo mod tidy\ngo run .\n```",
        ),

        // RUST + AXUM
        ("Rust", "Axum") => (
            3000,
            vec![
                ("src/main.rs", r#"
use axum::{routing::get, Json, Router};
use serde_json::{json, Value};
use std::net::SocketAddr;

async fn root() -> Json<Value> {
    Json(json!({
        "service": "SERVICE_NAME",
        "message": "API is running"
    }))
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/", get(root))
        .route("/health", get(health));

    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse()
        .expect("PORT must be a valid port number");

    let address = SocketAddr::from(([0, 0, 0, 0], port));

    let listener = tokio::net::TcpListener::bind(address)
        .await
        .expect("Could not bind to port");

    println!("SERVICE_NAME listening on {}", address);

    axum::serve(listener, app)
        .await
        .expect("Server error");
}
"#),
                ("Cargo.toml", r#"
[package]
name = "SERVICE_NAME"
version = "0.1.0"
edition = "2021"

[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full"] }
serde_json = "1"
"#),
                ("Dockerfile", r#"
FROM rust:slim AS builder
WORKDIR /app
COPY Cargo.toml ./
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=builder /app/target/release/SERVICE_NAME ./server
ENV PORT=3000
EXPOSE 3000
CMD ["./server"]
"#),
            ],
            "```powershell\ncargo run\n```",
        ),

        _ => {
            return Err("Unsupported language and framework combination.".into());
        }
    };

    let host_port = find_available_port()?;

    // Create the project directory only after validating the template.
    std::fs::create_dir(&service_dir)
        .map_err(|e| format!("Could not create service directory: {e}"))?;

    let result = (|| -> Result<(), String> {
        for (relative_path, contents) in &files {
            write_file(
                &service_dir,
                relative_path,
                &contents.replace("SERVICE_NAME", &name),
            )?;
        }

        let compose = format!(
            r#"services:
  {name}:
    build: .
    container_name: {name}
    ports:
      - "127.0.0.1:${{HOST_PORT}}:{container_port}"
    environment:
      PORT: "{container_port}"
    restart: unless-stopped
"#,
            name = name,
            container_port = container_port,
        );

        write_file(&service_dir, "docker-compose.yml", &compose)?;

        let env_file = format!(
            "HOST_PORT={}\nPORT={}\n",
            host_port,
            container_port
        );

        write_file(&service_dir, ".env", &env_file)?;

        write_file(
            &service_dir,
            ".gitignore",
            r#".env
node_modules/
.venv/
__pycache__/
*.pyc
target/
"#,
        )?;

        write_file(
            &service_dir,
            ".dockerignore",
            r#".git
.env
.micronom
.micronom
node_modules
.venv
__pycache__
*.pyc
target
"#,
        )?;

        let readme = format!(
            r#"# {name}

Generated by Micronom.

## Run locally

{local_instructions}

## Run with Docker

Make sure Docker Desktop is running.

```powershell
docker compose up --build -d
```

## Stop the service

```powershell
docker compose down
```

## API endpoints

- GET / - Service information
- GET /health - Health check

## Addresses

Local development: http://localhost:{container_port}

Docker: http://localhost:{host_port}

## Configuration

The `.env` file contains the host and container ports.

Change `HOST_PORT` in `.env` if the selected host port is already in use.
"#,
            name = name,
            local_instructions = local_instructions,
            container_port = container_port,
            host_port = host_port,
        );

        write_file(&service_dir, "README.md", &readme)?;

        Ok(())
    })();

    if let Err(error) = result {
        let _ = std::fs::remove_dir_all(&service_dir);
        return Err(error);
    }

    Ok(service_dir.to_string_lossy().into_owned())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            create_service_folder,
            configure_service_database,
            configure_message_connection,
            configure_grpc_connection,
            list_service_folders,
            list_service_connections,
            suggest_service_command,
            execute_service_command,
            remove_service_folder,
            run_all_services,
            stop_all_services
        ])
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while building tauri application");
}

#[tauri::command]
fn remove_service_folder(name: String) -> Result<(), String> {
    use std::fs;

    // Only allow safe service folder names.
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("Invalid microservice name.".to_string());
    }

    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .ok_or_else(|| "Could not find the user profile directory.".to_string())?;

    let app_dir = PathBuf::from(home).join("app");

    let canonical_app = app_dir
        .canonicalize()
        .map_err(|_| "Micronom app directory does not exist.".to_string())?;

    let service_path = canonical_app.join(&name);

    let canonical_service = service_path
        .canonicalize()
        .map_err(|_| "Microservice folder was not found.".to_string())?;

    // Only allow deleting a direct child of the app directory.
    if canonical_service.parent() != Some(canonical_app.as_path()) {
        return Err("Invalid microservice directory.".to_string());
    }

    if !canonical_service.is_dir() {
        return Err("The microservice path is not a directory.".to_string());
    }

    let connections = read_service_connections(&canonical_app)?;
    let original_connection_count = connections.len();
    let remaining_connections = connections
        .into_iter()
        .filter(|connection| connection.source_service != name && connection.target_service != name)
        .collect::<Vec<_>>();
    let connections_changed = remaining_connections.len() != original_connection_count;

    fs::remove_dir_all(&canonical_service)
        .map_err(|error| format!("Failed to delete microservice: {}", error))?;

    if connections_changed {
        sync_service_connection_artifacts(&canonical_app, &remaining_connections)?;
        write_service_connections(&canonical_app, &remaining_connections)?;
    }

    Ok(())
}

fn write_environment_file(
    directory: &Path,
    file_name: &str,
    entries: &[(&str, String)],
    replace_keys: &[&str],
) -> Result<(), String> {
    let path = directory.join(file_name);
    let existing = match std::fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(format!("Could not read {file_name}: {error}")),
    };

    let mut lines = existing
        .lines()
        .filter(|line| {
            let key = line.split_once('=').map(|(key, _)| key.trim());
            !key.is_some_and(|key| replace_keys.contains(&key))
        })
        .map(str::to_string)
        .collect::<Vec<_>>();

    lines.extend(entries.iter().map(|(key, value)| format!("{key}={value}")));
    write_file(directory, file_name, &format!("{}\n", lines.join("\n")))
}

fn write_database_environment(
    service_dir: &Path,
    entries: &[(&str, String)],
) -> Result<(), String> {
    let database_keys = [
        "DATABASE_TYPE",
        "DATABASE_URL",
        "MONGO_USER",
        "MONGO_PASSWORD",
        "POSTGRES_PASSWORD",
        "REDIS_PASSWORD",
        "FAISS_INDEX_PATH",
    ];
    write_environment_file(service_dir, ".env", entries, &database_keys)
}

const MESSAGE_CONNECTION_ENV_KEYS: &[&str] = &[
    "RABBITMQ_URL",
    "KAFKA_BOOTSTRAP_SERVERS",
    "REDIS_STREAMS_URL",
];
const BROKER_SECRET_ENV_KEYS: &[&str] = &[
    "RABBITMQ_USER",
    "RABBITMQ_PASSWORD",
    "REDIS_STREAMS_PASSWORD",
];

fn read_environment_value(contents: &str, key: &str) -> Option<String> {
    contents.lines().find_map(|line| {
        let (entry_key, value) = line.split_once('=')?;
        (entry_key.trim() == key).then(|| value.trim().to_string())
    })
}

fn read_optional_environment_file(path: &Path) -> Result<String, String> {
    match std::fs::read_to_string(path) {
        Ok(contents) => Ok(contents),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(format!("Could not read {}: {error}", path.display())),
    }
}

fn add_node_dependencies(service_dir: &Path, dependencies: &[(&str, &str)]) -> Result<(), String> {
    let path = service_dir.join("package.json");
    let contents = std::fs::read_to_string(&path)
        .map_err(|error| format!("Could not read package.json: {error}"))?;
    let mut manifest: serde_json::Value = serde_json::from_str(&contents)
        .map_err(|error| format!("Could not parse package.json: {error}"))?;
    let manifest = manifest
        .as_object_mut()
        .ok_or_else(|| "The package.json root must be an object.".to_string())?;
    let dependency_map = manifest
        .entry("dependencies")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| "The package.json dependencies must be an object.".to_string())?;

    let mut changed = false;
    for (name, version) in dependencies {
        if !dependency_map.contains_key(*name) {
            dependency_map.insert(
                (*name).to_string(),
                serde_json::Value::String((*version).to_string()),
            );
            changed = true;
        }
    }

    if changed {
        let updated = serde_json::to_string_pretty(&manifest)
            .map_err(|error| format!("Could not update package.json: {error}"))?;
        write_file(service_dir, "package.json", &format!("{updated}\n"))?;
    }
    Ok(())
}

fn add_python_dependencies(
    service_dir: &Path,
    dependencies: &[(&str, &str)],
) -> Result<(), String> {
    let path = service_dir.join("requirements.txt");
    let mut contents = std::fs::read_to_string(&path)
        .map_err(|error| format!("Could not read requirements.txt: {error}"))?;
    let mut changed = false;

    for (name, requirement) in dependencies {
        let normalized_name = name.replace('_', "-").to_ascii_lowercase();
        let already_present = contents.lines().any(|line| {
            let package_name = line
                .trim()
                .split(|character: char| {
                    matches!(character, '<' | '>' | '=' | '!' | '[' | ';' | ' ' | '\t')
                })
                .next()
                .unwrap_or_default()
                .replace('_', "-")
                .to_ascii_lowercase();
            package_name == normalized_name
        });
        if !already_present {
            if !contents.is_empty() && !contents.ends_with('\n') {
                contents.push('\n');
            }
            contents.push_str(requirement);
            contents.push('\n');
            changed = true;
        }
    }

    if changed {
        write_file(service_dir, "requirements.txt", &contents)?;
    }
    Ok(())
}

fn add_go_dependencies(service_dir: &Path, dependencies: &[(&str, &str)]) -> Result<(), String> {
    let path = service_dir.join("go.mod");
    let mut contents =
        std::fs::read_to_string(&path).map_err(|error| format!("Could not read go.mod: {error}"))?;
    let mut changed = false;

    for (module, version) in dependencies {
        let already_present = contents.lines().any(|line| {
            let line = line.trim();
            line.strip_prefix("require ")
                .unwrap_or(line)
                .split_whitespace()
                .next()
                == Some(*module)
        });
        if !already_present {
            if !contents.is_empty() && !contents.ends_with('\n') {
                contents.push('\n');
            }
            contents.push_str(&format!("\nrequire {module} {version}\n"));
            changed = true;
        }
    }

    if changed {
        write_file(service_dir, "go.mod", &contents)?;
    }
    Ok(())
}

fn add_cargo_dependencies(service_dir: &Path, dependencies: &[(&str, &str)]) -> Result<(), String> {
    let path = service_dir.join("Cargo.toml");
    let contents = std::fs::read_to_string(&path)
        .map_err(|error| format!("Could not read Cargo.toml: {error}"))?;
    let mut lines = contents.lines().map(str::to_string).collect::<Vec<_>>();
    let section_start = lines
        .iter()
        .position(|line| line.trim() == "[dependencies]")
        .ok_or_else(|| "Cargo.toml has no [dependencies] section.".to_string())?;
    let section_end = lines[section_start + 1..]
        .iter()
        .position(|line| line.trim_start().starts_with('['))
        .map(|offset| section_start + 1 + offset)
        .unwrap_or(lines.len());

    let mut missing = Vec::new();
    for (name, specification) in dependencies {
        let exists = lines[section_start + 1..section_end].iter().any(|line| {
            line.split_once('=')
                .is_some_and(|(key, _)| key.trim() == *name)
        });
        if !exists {
            missing.push(format!("{name} = {specification}"));
        }
    }

    if !missing.is_empty() {
        lines.splice(section_end..section_end, missing);
        write_file(
            service_dir,
            "Cargo.toml",
            &format!("{}\n", lines.join("\n")),
        )?;
    }
    Ok(())
}

fn ensure_service_dependencies(
    service_dir: &Path,
    rabbitmq: bool,
    grpc: bool,
) -> Result<(), String> {
    let language = detect_service_language(service_dir)
        .map(|(language, _)| language)
        .ok_or_else(|| "Could not determine the microservice language.".to_string())?;

    match language {
        "Node.js" => {
            let mut dependencies = Vec::new();
            if rabbitmq {
                dependencies.push(("amqplib", "^0.10.8"));
            }
            if grpc {
                dependencies.extend([
                    ("@grpc/grpc-js", "^1.13.4"),
                    ("@grpc/proto-loader", "^0.7.13"),
                ]);
            }
            add_node_dependencies(service_dir, &dependencies)
        }
        "Python" => {
            let mut dependencies = Vec::new();
            if rabbitmq {
                dependencies.push(("pika", "pika>=1.3.2,<2"));
            }
            if grpc {
                dependencies.push(("grpcio", "grpcio>=1.71,<2"));
            }
            add_python_dependencies(service_dir, &dependencies)
        }
        "Go" => {
            let mut dependencies = Vec::new();
            if rabbitmq {
                dependencies.push(("github.com/rabbitmq/amqp091-go", "v1.10.0"));
            }
            if grpc {
                dependencies.extend([
                    ("google.golang.org/grpc", "v1.71.0"),
                    ("google.golang.org/protobuf", "v1.36.6"),
                ]);
            }
            add_go_dependencies(service_dir, &dependencies)
        }
        "Rust" => {
            let mut dependencies = Vec::new();
            if rabbitmq {
                dependencies.push(("lapin", "\"2.5\""));
            }
            if grpc {
                dependencies.extend([("tonic", "\"0.13\""), ("prost", "\"0.13\"")]);
            }
            add_cargo_dependencies(service_dir, &dependencies)
        }
        _ => Err("Unsupported microservice language.".into()),
    }
}

fn grpc_target_environment_key(target_service: &str) -> String {
    format!(
        "GRPC_TARGET_{}",
        target_service.replace('-', "_").to_ascii_uppercase()
    )
}

fn write_connection_environment(
    service_dir: &Path,
    entries: &[(String, String)],
) -> Result<(), String> {
    let path = service_dir.join(".env");
    let existing = read_optional_environment_file(&path)?;
    let mut lines = existing
        .lines()
        .filter(|line| {
            let Some((key, _)) = line.split_once('=') else {
                return true;
            };
            let key = key.trim();
            !MESSAGE_CONNECTION_ENV_KEYS.contains(&key)
                && key != "GRPC_LISTEN_ADDRESS"
                && !key.starts_with("GRPC_TARGET_")
        })
        .map(str::to_string)
        .collect::<Vec<_>>();
    lines.extend(entries.iter().map(|(key, value)| format!("{key}={value}")));
    write_file(service_dir, ".env", &format!("{}\n", lines.join("\n")))
}

fn used_brokers(connections: &[ServiceConnection]) -> Result<Vec<BrokerType>, String> {
    let mut brokers = Vec::new();
    for connection in connections {
        match (connection.method, connection.broker) {
            (ServiceConnectionType::Messaging, Some(broker)) => {
                if !brokers.contains(&broker) {
                    brokers.push(broker);
                }
            }
            (ServiceConnectionType::Messaging, None) => {
                return Err("A messaging connection is missing its broker.".into());
            }
            (ServiceConnectionType::Grpc, Some(_)) => {
                return Err("A gRPC connection cannot specify a message broker.".into());
            }
            (ServiceConnectionType::Grpc, None) => {}
        }
    }
    brokers.sort_by_key(|broker| broker.as_str());
    Ok(brokers)
}

fn generate_workspace_broker_compose(brokers: &[BrokerType]) -> String {
    let mut services = String::new();
    let mut volumes = Vec::new();

    for broker in brokers {
        match broker {
            BrokerType::RabbitMq => {
                services.push_str(
                    r#"  rabbitmq:
    image: rabbitmq:4-management
    environment:
      RABBITMQ_DEFAULT_USER: "${RABBITMQ_USER}"
      RABBITMQ_DEFAULT_PASS: "${RABBITMQ_PASSWORD}"
    volumes:
      - rabbitmq-data:/var/lib/rabbitmq
    networks:
      - micronom-services
    healthcheck:
      test: ["CMD", "rabbitmq-diagnostics", "-q", "ping"]
      interval: 5s
      timeout: 5s
      retries: 12
    restart: unless-stopped
"#,
                );
                volumes.push("  rabbitmq-data: {}\n");
            }
            BrokerType::Kafka => {
                services.push_str(
                    r#"  kafka:
    image: apache/kafka:4.1.1
    environment:
      KAFKA_NODE_ID: 1
      KAFKA_PROCESS_ROLES: "broker,controller"
      KAFKA_LISTENERS: "PLAINTEXT://0.0.0.0:9092,CONTROLLER://0.0.0.0:9093"
      KAFKA_ADVERTISED_LISTENERS: "PLAINTEXT://kafka:9092"
      KAFKA_CONTROLLER_LISTENER_NAMES: CONTROLLER
      KAFKA_LISTENER_SECURITY_PROTOCOL_MAP: "CONTROLLER:PLAINTEXT,PLAINTEXT:PLAINTEXT"
      KAFKA_CONTROLLER_QUORUM_VOTERS: "1@kafka:9093"
      KAFKA_OFFSETS_TOPIC_REPLICATION_FACTOR: 1
      KAFKA_TRANSACTION_STATE_LOG_REPLICATION_FACTOR: 1
      KAFKA_TRANSACTION_STATE_LOG_MIN_ISR: 1
      KAFKA_GROUP_INITIAL_REBALANCE_DELAY_MS: 0
      KAFKA_NUM_PARTITIONS: 3
      KAFKA_LOG_DIRS: /var/lib/kafka/data
    volumes:
      - kafka-data:/var/lib/kafka/data
    networks:
      - micronom-services
    healthcheck:
      test: ["CMD-SHELL", "/opt/kafka/bin/kafka-topics.sh --bootstrap-server localhost:9092 --list >/dev/null 2>&1"]
      interval: 10s
      timeout: 10s
      retries: 18
    restart: unless-stopped
"#,
                );
                volumes.push("  kafka-data: {}\n");
            }
            BrokerType::RedisStreams => {
                services.push_str(
                    r#"  redis-streams:
    image: redis:7-alpine
    environment:
      REDIS_STREAMS_PASSWORD: "${REDIS_STREAMS_PASSWORD}"
      REDISCLI_AUTH: "${REDIS_STREAMS_PASSWORD}"
    command: ["redis-server", "--appendonly", "yes", "--requirepass", "${REDIS_STREAMS_PASSWORD}"]
    volumes:
      - redis-streams-data:/data
    networks:
      - micronom-services
    healthcheck:
      test: ["CMD", "redis-cli", "ping"]
      interval: 5s
      timeout: 5s
      retries: 12
    restart: unless-stopped
"#,
                );
                volumes.push("  redis-streams-data: {}\n");
            }
        }
    }

    format!(
        "services:\n{services}networks:\n  micronom-services:\n    external: true\n    name: micronom-services\nvolumes:\n{}",
        volumes.join("")
    )
}

fn generate_service_connection_compose(service_name: &str, environment_keys: &[String]) -> String {
    let environment = environment_keys
        .iter()
        .map(|key| format!("      {key}: \"${{{key}}}\""))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"services:
  {service_name}:
    environment:
{environment}
    networks:
      - micronom-services
networks:
  micronom-services:
    external: true
    name: micronom-services
"#
    )
}

fn workspace_broker_secrets(
    app_dir: &Path,
    brokers: &[BrokerType],
) -> Result<HashMap<BrokerType, String>, String> {
    let broker_dir = app_dir.join(".micronom").join("brokers");
    std::fs::create_dir_all(&broker_dir)
        .map_err(|error| format!("Could not create workspace broker directory: {error}"))?;

    let env_path = broker_dir.join(".env");
    let existing = read_optional_environment_file(&env_path)?;
    let mut entries = Vec::new();
    let mut secrets = HashMap::new();

    for broker in brokers {
        match broker {
            BrokerType::RabbitMq | BrokerType::RedisStreams => {
                let key = broker.secret_key().ok_or_else(|| {
                    format!("{} credentials are not configured.", broker.as_str())
                })?;
                let password = match read_environment_value(&existing, key) {
                    Some(value)
                        if value.len() == 64
                            && value.chars().all(|character| character.is_ascii_hexdigit()) =>
                    {
                        value
                    }
                    Some(_) => {
                        return Err(format!(
                            "Invalid stored credentials for {}.",
                            broker.as_str()
                        ));
                    }
                    None => generate_database_password()?,
                };
                if *broker == BrokerType::RabbitMq {
                    entries.push(("RABBITMQ_USER", "micronom".to_string()));
                }
                entries.push((key, password.clone()));
                secrets.insert(*broker, password);
            }
            BrokerType::Kafka => {}
        }
    }

    write_environment_file(&broker_dir, ".env", &entries, BROKER_SECRET_ENV_KEYS)?;
    write_file(&broker_dir, ".gitignore", ".env\n")?;
    write_file(
        &broker_dir,
        "docker-compose.yml",
        &generate_workspace_broker_compose(brokers),
    )?;

    Ok(secrets)
}

fn append_service_connection_documentation(service_dir: &Path) -> Result<(), String> {
    let path = service_dir.join("README.md");
    let existing = std::fs::read_to_string(&path)
        .map_err(|error| format!("Could not read the service README: {error}"))?;
    if existing
        .lines()
        .any(|line| line.trim() == "## Service connections")
    {
        return Ok(());
    }

    let section = r#"
## Service connections

RabbitMQ connections add the language client dependency and set `RABBITMQ_URL`.
gRPC connections add the language runtime dependencies, set `GRPC_LISTEN_ADDRESS`
on the destination, and set a `GRPC_TARGET_<SERVICE>` address on the source.
Run all installs these dependencies during the Docker image build. These addresses
use Micronom's shared Docker network. Micronom does not generate publisher/consumer
or RPC handler code; add that application logic as needed.
"#;
    write_file(service_dir, "README.md", &format!("{existing}{section}"))
}

fn sync_service_connection_artifacts(
    app_dir: &Path,
    connections: &[ServiceConnection],
) -> Result<(), String> {
    let brokers = used_brokers(connections)?;
    let secrets = if brokers.is_empty() {
        HashMap::new()
    } else {
        workspace_broker_secrets(app_dir, &brokers)?
    };

    let mut service_brokers: HashMap<String, Vec<BrokerType>> = HashMap::new();
    let mut grpc_targets: HashMap<String, Vec<String>> = HashMap::new();
    let mut grpc_services = HashSet::new();
    let mut expected_services = Vec::new();
    for connection in connections {
        for service_name in [&connection.source_service, &connection.target_service] {
            if !expected_services.contains(service_name) {
                expected_services.push(service_name.clone());
            }
        }

        match (connection.method, connection.broker) {
            (ServiceConnectionType::Messaging, Some(broker)) => {
                for service_name in [&connection.source_service, &connection.target_service] {
                    let service_broker_types =
                        service_brokers.entry(service_name.clone()).or_default();
                    if !service_broker_types.contains(&broker) {
                        service_broker_types.push(broker);
                    }
                }
            }
            (ServiceConnectionType::Grpc, None) => {
                let targets = grpc_targets
                    .entry(connection.source_service.clone())
                    .or_default();
                if !targets.contains(&connection.target_service) {
                    targets.push(connection.target_service.clone());
                }
                grpc_services.insert(connection.source_service.clone());
                grpc_services.insert(connection.target_service.clone());
            }
            (ServiceConnectionType::Messaging, None) => {
                return Err("A messaging connection is missing its broker.".into());
            }
            (ServiceConnectionType::Grpc, Some(_)) => {
                return Err("A gRPC connection cannot specify a message broker.".into());
            }
        }
    }

    let entries = std::fs::read_dir(app_dir)
        .map_err(|error| format!("Could not read the app directory: {error}"))?;
    let mut found_services = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("Could not read an app entry: {error}"))?;
        if !entry
            .file_type()
            .map_err(|error| format!("Could not inspect an app entry: {error}"))?
            .is_dir()
        {
            continue;
        }

        let service_dir = entry.path();
        if !service_dir.join("docker-compose.yml").is_file() {
            continue;
        }
        let service_name = entry.file_name().to_string_lossy().into_owned();
        found_services.push(service_name.clone());
        let service_broker_types = service_brokers.get(&service_name);
        let service_grpc_targets = grpc_targets.get(&service_name);
        let has_grpc = grpc_services.contains(&service_name);
        let has_connection = service_broker_types.is_some() || has_grpc;
        let env_path = service_dir.join(".env");
        if has_connection && !env_path.is_file() {
            return Err(format!(
                "The environment file is missing for service {service_name}."
            ));
        }

        if has_connection {
            let rabbitmq = service_broker_types
                .is_some_and(|service_brokers| service_brokers.contains(&BrokerType::RabbitMq));
            ensure_service_dependencies(&service_dir, rabbitmq, has_grpc)?;

            let mut env_entries = Vec::new();
            if let Some(service_broker_types) = service_broker_types {
                for broker in service_broker_types {
                    let secret = secrets.get(broker).map(String::as_str);
                    env_entries.push((broker.env_key().to_string(), broker.connection_url(secret)?));
                }
            }
            if let Some(targets) = service_grpc_targets {
                for target in targets {
                    env_entries.push((
                        grpc_target_environment_key(target),
                        format!("{target}:50051"),
                    ));
                }
            }
            if has_grpc {
                env_entries.push(("GRPC_LISTEN_ADDRESS".into(), "0.0.0.0:50051".into()));
            }
            write_connection_environment(&service_dir, &env_entries)?;
            exclude_database_metadata_from_build_context(&service_dir)?;
            append_service_connection_documentation(&service_dir)?;
            let environment_keys = env_entries
                .iter()
                .map(|(key, _)| key.clone())
                .collect::<Vec<_>>();
            write_file(
                &service_dir,
                "docker-compose.connections.yml",
                &generate_service_connection_compose(&service_name, &environment_keys),
            )?;
        } else {
            if env_path.is_file() {
                write_connection_environment(&service_dir, &[])?;
            }
            match std::fs::remove_file(service_dir.join("docker-compose.connections.yml")) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(format!(
                        "Could not remove messaging configuration for {service_name}: {error}"
                    ));
                }
            }
        }
    }

    if let Some(missing) = expected_services
        .iter()
        .find(|service_name| !found_services.contains(service_name))
    {
        return Err(format!(
            "Connected service {missing} does not have a Docker Compose file."
        ));
    }

    Ok(())
}

fn is_valid_service_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 48
        && name.as_bytes()[0].is_ascii_alphanumeric()
        && name.as_bytes()[name.len() - 1].is_ascii_alphanumeric()
        && name.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
}

fn resolve_service_directory(app_dir: &Path, service_name: &str) -> Result<PathBuf, String> {
    if !is_valid_service_name(service_name) {
        return Err("Invalid microservice name.".into());
    }

    let canonical_app = app_dir
        .canonicalize()
        .map_err(|error| format!("Could not access the app directory: {error}"))?;
    let canonical_service = app_dir
        .join(service_name)
        .canonicalize()
        .map_err(|error| format!("Could not access service {service_name}: {error}"))?;
    if canonical_service.parent() != Some(canonical_app.as_path())
        || !canonical_service.is_dir()
        || !canonical_service.join("docker-compose.yml").is_file()
    {
        return Err(format!("Invalid microservice directory: {service_name}."));
    }

    Ok(canonical_service)
}

fn configure_service_connection(connection: ServiceConnection) -> Result<(), String> {
    if connection.id == 0 {
        return Err("Invalid connection identifier.".into());
    }
    if connection.source_service == connection.target_service {
        return Err("A service cannot connect to itself.".into());
    }

    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .ok_or_else(|| "Could not find the user home directory.".to_string())?;
    let app_dir = PathBuf::from(home).join("app");
    resolve_service_directory(&app_dir, &connection.source_service)?;
    resolve_service_directory(&app_dir, &connection.target_service)?;

    let mut connections = read_service_connections(&app_dir)?;
    if connections.iter().any(|existing| {
        existing.id == connection.id
            || (existing.source_service == connection.source_service
                && existing.target_service == connection.target_service
                && existing.method == connection.method
                && existing.broker == connection.broker)
    }) {
        return Err("This service connection is already configured.".into());
    }

    connections.push(connection);
    sync_service_connection_artifacts(&app_dir, &connections)?;
    write_service_connections(&app_dir, &connections)
}

#[tauri::command]
fn configure_message_connection(
    connection_id: u64,
    source_service: String,
    target_service: String,
    broker: BrokerType,
) -> Result<(), String> {
    configure_service_connection(ServiceConnection {
        id: connection_id,
        source_service,
        target_service,
        method: ServiceConnectionType::Messaging,
        broker: Some(broker),
    })
}

#[tauri::command]
fn configure_grpc_connection(
    connection_id: u64,
    source_service: String,
    target_service: String,
) -> Result<(), String> {
    configure_service_connection(ServiceConnection {
        id: connection_id,
        source_service,
        target_service,
        method: ServiceConnectionType::Grpc,
        broker: None,
    })
}

fn add_faiss_dependency(service_dir: &Path) -> Result<(), String> {
    let requirements_path = service_dir.join("requirements.txt");
    let mut requirements = std::fs::read_to_string(&requirements_path)
        .map_err(|error| format!("Could not read Python requirements: {error}"))?;

    if !requirements
        .lines()
        .any(|line| line.trim().to_ascii_lowercase().starts_with("faiss-cpu"))
    {
        if !requirements.is_empty() && !requirements.ends_with('\n') {
            requirements.push('\n');
        }
        requirements.push_str("faiss-cpu>=1.8,<2\n");
        write_file(service_dir, "requirements.txt", &requirements)?;
    }

    Ok(())
}

fn prepare_faiss_runtime(service_dir: &Path) -> Result<(), String> {
    add_faiss_dependency(service_dir)?;

    let dockerfile = std::fs::read_to_string(service_dir.join("Dockerfile"))
        .map_err(|error| format!("Could not read the Python Dockerfile: {error}"))?;
    if dockerfile.contains("libgomp1") {
        return Ok(());
    }

    let mut updated_lines = Vec::new();
    let mut added_runtime_library = false;
    for line in dockerfile.lines() {
        updated_lines.push(line.to_string());
        if line.trim() == "FROM python:3.12-slim" {
            updated_lines.push(
                "RUN apt-get update && apt-get install -y --no-install-recommends libgomp1 && rm -rf /var/lib/apt/lists/*"
                    .into(),
            );
            added_runtime_library = true;
        }
    }

    if !added_runtime_library {
        return Err("Could not configure FAISS in the Python Dockerfile.".into());
    }

    write_file(
        service_dir,
        "Dockerfile",
        &format!("{}\n", updated_lines.join("\n")),
    )
}

fn append_database_documentation(
    service_dir: &Path,
    database_type: DatabaseType,
) -> Result<(), String> {
    let path = service_dir.join("README.md");
    let existing = std::fs::read_to_string(&path)
        .map_err(|error| format!("Could not read the service README: {error}"))?;
    if existing.lines().any(|line| line.trim() == "## Database") {
        return Ok(());
    }
    let database_details = match database_type {
        DatabaseType::Faiss => {
            "FAISS is embedded in the Python service. The `faiss-cpu` package and `FAISS_INDEX_PATH` are configured, and the index directory is persisted in a Docker volume. Add indexing and search logic in the application to use it.".to_string()
        }
        _ => format!(
            "{} runs as a companion container. The service receives its connection string in `DATABASE_URL`; add a client driver and connection logic in the application to use it.",
            database_type.label()
        ),
    };
    let section = format!(
        "\n## Database\n\n{database_details}\n\nCredentials are stored in `.env`, which is ignored by Git. Run all starts the service and its database; shutting down keeps the named database volume.\n"
    );

    write_file(service_dir, "README.md", &format!("{existing}{section}"))
}

fn exclude_database_metadata_from_build_context(service_dir: &Path) -> Result<(), String> {
    let path = service_dir.join(".dockerignore");
    let existing = std::fs::read_to_string(&path)
        .map_err(|error| format!("Could not read the Docker ignore file: {error}"))?;
    if existing.lines().any(|line| line.trim() == ".micronom") {
        return Ok(());
    }

    write_file(
        service_dir,
        ".dockerignore",
        &format!("{}.micronom\n", existing.trim_end()),
    )
}

fn prepare_mongo_database(service_dir: &Path, service_name: &str) -> Result<(), String> {
    let init_script = format!(
        "db = db.getSiblingDB(\"{service_name}\");\ndb.createCollection(\"_micronom_init\");\n"
    );
    write_file(service_dir, ".micronom/mongo-init.js", &init_script)
}

#[tauri::command]
fn configure_service_database(
    service_name: String,
    database_type: DatabaseType,
) -> Result<DatabaseType, String> {
    if service_name.is_empty()
        || service_name.len() > 48
        || !service_name.as_bytes()[0].is_ascii_alphanumeric()
        || !service_name.as_bytes()[service_name.len() - 1].is_ascii_alphanumeric()
        || !service_name.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
    {
        return Err("Invalid microservice name.".into());
    }

    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .ok_or_else(|| "Could not find the user home directory.".to_string())?;
    let app_dir = PathBuf::from(home).join("app");
    let canonical_app = app_dir
        .canonicalize()
        .map_err(|error| format!("Could not access the Micronom app directory: {error}"))?;
    let canonical_service = app_dir
        .join(&service_name)
        .canonicalize()
        .map_err(|error| format!("Could not access the microservice directory: {error}"))?;

    if canonical_service.parent() != Some(canonical_app.as_path()) || !canonical_service.is_dir()
    {
        return Err("Invalid microservice directory.".into());
    }
    if !canonical_service.join("docker-compose.yml").is_file() {
        return Err("The microservice does not have a Docker Compose file.".into());
    }
    if database_type == DatabaseType::Faiss
        && !canonical_service.join("requirements.txt").is_file()
    {
        return Err("FAISS is currently supported only for Python services.".into());
    }
    if read_database_type(&canonical_service)?.is_some() {
        return Err("This microservice already has a database configured.".into());
    }

    let password = if database_type == DatabaseType::Faiss {
        None
    } else {
        Some(generate_database_password()?)
    };
    let environment =
        database_env_entries(&service_name, database_type, password.as_deref())?;
    let compose = generate_database_compose(&service_name, database_type);

    match database_type {
        DatabaseType::Mongo => prepare_mongo_database(&canonical_service, &service_name)?,
        DatabaseType::Faiss => prepare_faiss_runtime(&canonical_service)?,
        DatabaseType::Postgres | DatabaseType::Redis => {}
    }

    write_database_environment(&canonical_service, &environment)?;
    exclude_database_metadata_from_build_context(&canonical_service)?;
    write_file(&canonical_service, "docker-compose.db.yml", &compose)?;
    append_database_documentation(&canonical_service, database_type)?;

    let metadata = serde_json::to_string(&database_type)
        .map_err(|error| format!("Could not save database configuration: {error}"))?;
    write_file(
        &canonical_service,
        ".micronom/database.json",
        &format!("{metadata}\n"),
    )?;

    Ok(database_type)
}


#[derive(serde::Serialize)]
struct ServiceActionResult {
    name: String,
    success: bool,
    message: String,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ServiceActionProgress {
    name: String,
    message: String,
}

fn emit_service_progress(app: &tauri::AppHandle, name: &str, message: String) {
    if let Err(error) = app.emit(
        "service-action-progress",
        ServiceActionProgress {
            name: name.to_string(),
            message,
        },
    ) {
        log::warn!("Could not report Docker progress for {name}: {error}");
    }
}

fn forward_compose_output<R: Read + Send + 'static>(
    reader: R,
    app: tauri::AppHandle,
    name: String,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        for line in BufReader::new(reader).lines() {
            match line {
                Ok(line) if !line.trim().is_empty() => {
                    emit_service_progress(&app, &name, line);
                }
                Ok(_) => {}
                Err(error) => {
                    emit_service_progress(
                        &app,
                        &name,
                        format!("Could not read Docker output: {error}"),
                    );
                    break;
                }
            }
        }
    })
}

fn run_compose(
    app: &tauri::AppHandle,
    service_dir: &Path,
    name: &str,
    action: &str,
    wait_for_healthy: bool,
) -> Result<ExitStatus, String> {
    emit_service_progress(
        app,
        name,
        if action == "up" {
            "Building and starting containers...".into()
        } else {
            "Stopping containers...".into()
        },
    );

    let mut command = Command::new("docker");
    command.arg("compose");
    for compose_file in [
        "docker-compose.yml",
        "docker-compose.db.yml",
        "docker-compose.connections.yml",
    ] {
        if compose_file == "docker-compose.yml" || service_dir.join(compose_file).is_file() {
            command.args(["-f", compose_file]);
        }
    }

    if action == "up" {
        command.args([
            "--progress",
            "plain",
            "up",
            "--build",
            "--remove-orphans",
            "-d",
        ]);
        if wait_for_healthy {
            command.args(["--wait", "--wait-timeout", "180"]);
        }
    } else {
        command.args(["down", "--remove-orphans"]);
    }

    let mut child = command
        .current_dir(service_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Could not execute Docker: {error}"))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Could not capture Docker output.".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Could not capture Docker errors.".to_string())?;
    let stdout_reader = forward_compose_output(stdout, app.clone(), name.to_string());
    let stderr_reader = forward_compose_output(stderr, app.clone(), name.to_string());
    let started = Instant::now();

    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(format!("Could not monitor Docker: {error}"));
            }
        }

        if started.elapsed() >= DOCKER_COMMAND_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            break Err(format!(
                "Docker operation exceeded the {}-second limit.",
                DOCKER_COMMAND_TIMEOUT.as_secs()
            ));
        }

        thread::sleep(Duration::from_millis(100));
    };

    let _ = stdout_reader.join();
    let _ = stderr_reader.join();
    status
}

fn ensure_shared_service_network() -> Result<(), String> {
    let inspect = Command::new("docker")
        .args(["network", "inspect", "micronom-services"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| format!("Could not inspect Docker networks: {error}"))?;
    if inspect.success() {
        return Ok(());
    }

    let create = Command::new("docker")
        .args(["network", "create", "micronom-services"])
        .output()
        .map_err(|error| format!("Could not create the shared service network: {error}"))?;
    if create.status.success() {
        return Ok(());
    }

    let inspect_after_create = Command::new("docker")
        .args(["network", "inspect", "micronom-services"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| format!("Could not verify the shared service network: {error}"))?;
    if inspect_after_create.success() {
        return Ok(());
    }

    Err(format!(
        "Could not create the shared service network: {}",
        String::from_utf8_lossy(&create.stderr).trim()
    ))
}

fn manage_all_services(
    action: &str,
    app: &tauri::AppHandle,
) -> Result<Vec<ServiceActionResult>, String> {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .ok_or_else(|| "Could not find the user home directory.".to_string())?;

    let app_dir = PathBuf::from(home).join("app");

    if !app_dir.exists() {
        return Ok(Vec::new());
    }

    let canonical_app = app_dir
        .canonicalize()
        .map_err(|e| format!("Could not access app directory: {e}"))?;

    let entries = std::fs::read_dir(&canonical_app)
        .map_err(|e| format!("Could not read app directory: {e}"))?;

    let mut results = Vec::new();
    let service_connections = read_service_connections(&canonical_app)?;
    let broker_dir = canonical_app.join(".micronom").join("brokers");
    let broker_compose = broker_dir.join("docker-compose.yml");
    let has_messaging_connections = service_connections
        .iter()
        .any(|connection| connection.method == ServiceConnectionType::Messaging);

    if action == "up" && !service_connections.is_empty() {
        ensure_shared_service_network()?;
    }

    if action == "up" && has_messaging_connections {
        if !broker_compose.is_file() {
            return Err(
                "Messaging connections exist but the shared broker configuration is missing."
                    .into(),
            );
        }

        match run_compose(app, &broker_dir, "Workspace brokers", "up", true) {
            Ok(status) if status.success() => {
                emit_service_progress(
                    app,
                    "Workspace brokers",
                    "Shared brokers are healthy.".into(),
                );
            }
            Ok(status) => {
                let message = format!("Docker operation failed ({status}).");
                emit_service_progress(app, "Workspace brokers", message.clone());
                results.push(ServiceActionResult {
                    name: "Workspace brokers".into(),
                    success: false,
                    message,
                });
                return Ok(results);
            }
            Err(message) => {
                emit_service_progress(app, "Workspace brokers", message.clone());
                results.push(ServiceActionResult {
                    name: "Workspace brokers".into(),
                    success: false,
                    message,
                });
                return Ok(results);
            }
        }
    }

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                results.push(ServiceActionResult {
                    name: "Unknown".into(),
                    success: false,
                    message: error.to_string(),
                });
                continue;
            }
        };

        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }

        let service_dir = entry.path();
        let compose_file = service_dir.join("docker-compose.yml");

        if !compose_file.is_file() {
            continue;
        }

        let name = entry.file_name().to_string_lossy().into_owned();

        match run_compose(app, &service_dir, &name, action, false) {
            Ok(status) => {
                let success = status.success();
                let message = if success {
                    "Operation completed.".into()
                } else {
                    format!("Docker operation failed ({status}).")
                };
                emit_service_progress(app, &name, message.clone());
                results.push(ServiceActionResult {
                    name,
                    success,
                    message,
                });
            }
            Err(message) => {
                emit_service_progress(app, &name, message.clone());
                results.push(ServiceActionResult {
                    name,
                    success: false,
                    message,
                });
            }
        }
    }

    if action == "down" && broker_compose.is_file() {
        match run_compose(app, &broker_dir, "Workspace brokers", "down", false) {
            Ok(status) => {
                let success = status.success();
                let message = if success {
                    "Shared brokers stopped; their data volumes were kept.".into()
                } else {
                    format!("Docker operation failed ({status}).")
                };
                emit_service_progress(app, "Workspace brokers", message.clone());
                results.push(ServiceActionResult {
                    name: "Workspace brokers".into(),
                    success,
                    message,
                });
            }
            Err(message) => {
                emit_service_progress(app, "Workspace brokers", message.clone());
                results.push(ServiceActionResult {
                    name: "Workspace brokers".into(),
                    success: false,
                    message,
                });
            }
        }
    }

    Ok(results)
}

#[tauri::command]
async fn run_all_services(app: tauri::AppHandle) -> Result<Vec<ServiceActionResult>, String> {
    tauri::async_runtime::spawn_blocking(move || manage_all_services("up", &app))
        .await
        .map_err(|error| format!("Docker task failed: {error}"))?
}

#[tauri::command]
async fn stop_all_services(app: tauri::AppHandle) -> Result<Vec<ServiceActionResult>, String> {
    tauri::async_runtime::spawn_blocking(move || manage_all_services("down", &app))
        .await
        .map_err(|error| format!("Docker task failed: {error}"))?
}

