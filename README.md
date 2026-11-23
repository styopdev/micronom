# Micronom

**Build. Connect. Orchestrate.**

Micronom is an AI-powered UI based framework for creating, managing, and orchestrating microservices from a single workspace.

It provides a unified environment where developers can create microservices using different programming languages and frameworks, manage their projects, and eventually use AI to generate and modify service code.
<img width="800" alt="Capture6" src="https://github.com/user-attachments/assets/1af3f63a-fe58-4cd5-afb6-485935751b86" />

---

## Features

- **Desktop application:** Built with Tauri 2, React, TypeScript, and Vite.
- **Microservice management:** Create and manage microservices from a central dashboard with UI.
- **Multiple languages:** Planned support for Node.js, Python, Go, and Rust.
- **Automatic workspace creation:** Micronom creates an `app` directory in the user's home directory when the first service is created.
- **Persistent workspace:** Services and connections are restored when the app is reopened.
- **Independent projects:** Each microservice gets its own directory.
- **Docker integration:** Planned support for building and running services in Docker containers.
- **AI command assistant:** Uses a local Ollama model to recommend a limited development command for the selected service. Commands are shown for review and require confirmation before running.
<img width="500" alt="Capture2" src="https://github.com/user-attachments/assets/fc3705ad-bf63-4070-bab9-74de091464bc" />
<img width="500" alt="Capture7" src="https://github.com/user-attachments/assets/2abde1fb-abaa-4019-ad84-72cc5164f7d4" />

## Technology Stack

| Component | Technology |
|---|---|
| Desktop framework | Tauri 2 |
| Frontend | React |
| Language | TypeScript |
| Build tool | Vite |
| Desktop backend | Rust |
| Icons | Lucide React |
| Containerization | Docker |
| Local AI | Ollama (planned) |

---

## Requirements

Before developing Micronom, install the following:

### 1. Node.js

Install Node.js (LTS recommended).

https://nodejs.org/

Verify the installation:

```powershell
node --version
npm --version
```

### 2. Rust

Install Rust using rustup:

https://rustup.rs/

Verify:

```powershell
rustc --version
cargo --version
```

### 3. Microsoft C++ Build Tools

On Windows, install Visual Studio Build Tools with the **Desktop development with C++** workload.

https://visualstudio.microsoft.com/visual-cpp-build-tools/

### 4. Docker Desktop

Docker is required for the planned container-management functionality.

https://www.docker.com/products/docker-desktop/

Verify:

```powershell
docker --version
```

---

## Installation

Clone the repository:

```powershell
git clone <REPOSITORY_URL>
```

Navigate into the project:

```powershell
cd micronom
```

Install dependencies:

```powershell
npm install
```

Install the Tauri CLI and API if they are not already included:

```powershell
npm install -D @tauri-apps/cli
npm install @tauri-apps/api
```

Install the icon library:

```powershell
npm install lucide-react
```

---

## Running Micronom

### Run as a desktop application

Start Micronom in development mode:

```powershell
npx tauri dev
```

This starts the Vite development server, compiles the Rust backend, and opens the desktop application.

### Run the frontend in a browser

```powershell
npm run dev
```

Open:

http://localhost:5173

Note that desktop-specific functionality requires the Tauri application.

### Build for production

Build the frontend:

```powershell
npm run build
```

## Local AI command assistant

The command assistant uses [Ollama](https://ollama.com/) locally; prompts are sent to the Ollama service on your computer, not to a hosted AI provider.

1. Install and start Ollama.
2. Download the default model:

   ```powershell
   ollama pull qwen2.5-coder:7b
   ```

3. Run Micronom as a desktop app, select a microservice, enter a request in **AI command assistant**, and choose **Ask AI**.
4. Review the suggested command and choose **Run this command** to execute it in that service's directory.

The assistant only supports predefined commands for the service language. It does not run arbitrary model-generated shell text. Command execution is limited to 120 seconds and captured output is capped.
