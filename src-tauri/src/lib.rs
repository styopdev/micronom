
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

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ExistingService {
    name: String,
    language: String,
    framework: String,
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
        let metadata = if directory.join("package.json").is_file() {
            Some(("Node.js", "Express"))
        } else if directory.join("requirements.txt").is_file() {
            Some(("Python", "FastAPI"))
        } else if directory.join("go.mod").is_file() {
            Some(("Go", "Gin"))
        } else if directory.join("Cargo.toml").is_file() {
            Some(("Rust", "Axum"))
        } else {
            None
        };

        if let Some((language, framework)) = metadata {
            services.push(ExistingService {
                name,
                language: language.into(),
                framework: framework.into(),
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
    use super::{allowed_command_ids, command_for_id};

    #[test]
    fn command_allowlist_is_scoped_to_service_language() {
        assert!(allowed_command_ids("Go").unwrap().contains(&"go-test"));
        assert!(!allowed_command_ids("Go").unwrap().contains(&"rust-test"));
    }

    #[test]
    fn unknown_command_ids_are_rejected() {
        assert!(command_for_id("powershell").is_none());
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
            list_service_folders,
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

    let home = std::env::var("USERPROFILE")
        .map_err(|_| "Could not find the user profile directory.".to_string())?;

    let app_dir = std::path::PathBuf::from(home).join("app");

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

    fs::remove_dir_all(&canonical_service)
        .map_err(|error| format!("Failed to delete microservice: {}", error))?;

    Ok(())
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
    if action == "up" {
        command.args([
            "compose",
            "--progress",
            "plain",
            "-f",
            "docker-compose.yml",
            "up",
            "--build",
            "-d",
        ]);
    } else {
        command.args(["compose", "-f", "docker-compose.yml", "down"]);
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

        match run_compose(app, &service_dir, &name, action) {
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

