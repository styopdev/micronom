# Micronom

**Build. Connect. Orchestrate.**

Micronom is an AI-powered desktop application for creating, managing, and orchestrating microservices from a single workspace.

It provides a unified environment where developers can create microservices using different programming languages and frameworks, manage their projects, and eventually use AI to generate and modify service code.

---

## Features

- **Desktop application:** Built with Tauri 2, React, TypeScript, and Vite.
- **Microservice management:** Create and manage microservices from a central dashboard.
- **Multiple languages:** Planned support for Node.js, Python, Go, and Rust.
- **Automatic workspace creation:** Micronom creates an `app` directory in the user's home directory when the first service is created.
- **Independent projects:** Each microservice gets its own directory.
- **Docker integration:** Planned support for building and running services in Docker containers.
- **AI development assistant:** Planned integration with local and cloud AI providers, including Ollama.

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

Build