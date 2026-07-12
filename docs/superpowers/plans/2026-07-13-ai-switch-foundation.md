# AI Switch Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the Phase A foundation for a Tauri-based AI provider and official account switching app with batch-first listing, SQLite storage, import history, atomic config writes, and extension interfaces.

**Architecture:** The frontend is a React and TypeScript Vite app that calls typed Tauri commands through `src/lib/api`. The backend is a Rust Tauri 2 app with command, service, repository, importer, adapter, config writer, and security layers. SQLite is the business data source of truth, while `settings.json` stores device-level settings under `~/.ai-switch/`.

**Tech Stack:** Tauri 2, React 18, TypeScript, Vite, Tailwind CSS, Vitest, Testing Library, Rust, sqlx SQLite, serde, thiserror, uuid, chrono, keyring, tempfile, sha2.

## Global Constraints

- This project is open source and commercially usable; use MIT unless the user explicitly requests Apache-2.0 before implementation starts.
- Clean-room rule: public behavior, public documentation, and public file formats may be studied, but non-commercial source code from `cockpit-tools` must not be copied or translated.
- Default data directory: `~/.ai-switch/`.
- SQLite is the single source of truth for business data.
- Device-level settings are stored in JSON outside the database.
- Provider and official account secrets are referenced by `secret_ref`; regular business tables do not store raw API keys, access tokens, or refresh tokens.
- Phase A does not implement real OAuth, real quota API calls, complete target app switching, MCP, prompts, skills, proxy, cloud sync, usage dashboard, session manager, updater, system tray hot-switching, multi-instance launching, or wakeup automation.
- Frontend styling uses Tailwind CSS with a small local component layer.
- SQLite access uses `sqlx` with migrations and repository wrappers.
- Secret storage uses the Rust `keyring` crate first; encrypted local fallback requires a visible reduced-security message.
- Release packaging automation is deferred; Phase A includes development, test, and local build scripts.

---

## File Structure

Create this structure during implementation:

```text
.
├── .gitignore
├── LICENSE
├── README.md
├── index.html
├── package.json
├── postcss.config.cjs
├── tailwind.config.ts
├── tsconfig.json
├── tsconfig.node.json
├── vite.config.ts
├── vitest.config.ts
├── src/
│   ├── App.tsx
│   ├── main.tsx
│   ├── styles.css
│   ├── components/
│   │   ├── layout/AppLayout.tsx
│   │   ├── batches/BatchList.tsx
│   │   ├── imports/ImportPanel.tsx
│   │   └── ui/Button.tsx
│   ├── lib/
│   │   ├── api/client.ts
│   │   ├── api/types.ts
│   │   └── query/queryClient.ts
│   ├── screens/
│   │   ├── AccountsScreen.tsx
│   │   ├── BatchesScreen.tsx
│   │   ├── DashboardScreen.tsx
│   │   ├── ImportsScreen.tsx
│   │   ├── OperationLogScreen.tsx
│   │   ├── ProvidersScreen.tsx
│   │   ├── SettingsScreen.tsx
│   │   └── TargetsScreen.tsx
│   └── test/
│       ├── setup.ts
│       └── fixtures.ts
├── tests/
│   ├── BatchList.test.tsx
│   ├── ImportPanel.test.tsx
│   └── SettingsScreen.test.tsx
├── fixtures/
│   └── example-import.json
└── src-tauri/
    ├── Cargo.toml
    ├── build.rs
    ├── tauri.conf.json
    ├── migrations/
    │   └── 202607130001_foundation.sql
    └── src/
        ├── main.rs
        ├── lib.rs
        ├── app_state.rs
        ├── error.rs
        ├── paths.rs
        ├── adapters/
        │   └── mod.rs
        ├── commands/
        │   ├── mod.rs
        │   ├── batch_commands.rs
        │   ├── import_commands.rs
        │   ├── settings_commands.rs
        │   └── target_commands.rs
        ├── config_writer/
        │   └── mod.rs
        ├── database/
        │   ├── mod.rs
        │   ├── repositories/
        │   │   ├── mod.rs
        │   │   ├── account_repository.rs
        │   │   ├── batch_repository.rs
        │   │   ├── import_repository.rs
        │   │   ├── provider_repository.rs
        │   │   └── target_repository.rs
        │   └── test_support.rs
        ├── importers/
        │   ├── mod.rs
        │   └── example_json.rs
        ├── models/
        │   ├── mod.rs
        │   ├── account.rs
        │   ├── batch.rs
        │   ├── import_job.rs
        │   ├── provider.rs
        │   ├── settings.rs
        │   └── target_app.rs
        ├── security/
        │   └── mod.rs
        └── services/
            ├── mod.rs
            ├── batch_service.rs
            ├── import_service.rs
            ├── settings_service.rs
            └── target_service.rs
```

Each backend module should own one boundary:

- `models`: serializable data shapes shared by repositories, services, and commands.
- `database/repositories`: SQL and persistence operations only.
- `services`: business rules, validation, grouping, import orchestration, and settings logic.
- `commands`: thin Tauri IPC wrappers.
- `importers`: parse external data into normalized records.
- `config_writer`: atomic writes and file hashing.
- `adapters`: target-app extension trait and mock adapter.
- `security`: secret store trait and keyring-backed implementation.

---

### Task 1: Scaffold The Tauri React Application

**Files:**
- Create: `package.json`
- Create: `.gitignore`
- Create: `LICENSE`
- Create: `index.html`
- Create: `tsconfig.json`
- Create: `tsconfig.node.json`
- Create: `vite.config.ts`
- Create: `vitest.config.ts`
- Create: `tailwind.config.ts`
- Create: `postcss.config.cjs`
- Create: `src/main.tsx`
- Create: `src/App.tsx`
- Create: `src/styles.css`
- Create: `src/components/layout/AppLayout.tsx`
- Create: `src/components/ui/Button.tsx`
- Create: `src-tauri/Cargo.toml`
- Create: `src-tauri/build.rs`
- Create: `src-tauri/tauri.conf.json`
- Create: `src-tauri/src/main.rs`
- Create: `src-tauri/src/lib.rs`

**Interfaces:**
- Produces: `run()` in `src-tauri/src/lib.rs`.
- Produces: `AppLayout` React component accepting `children: React.ReactNode`.
- Produces: npm scripts `dev`, `tauri:dev`, `build`, `typecheck`, `test`, `test:run`, `rust:check`, and `rust:test`.

- [ ] **Step 1: Create frontend package and config files**

Write `package.json`:

```json
{
  "name": "ai-switch",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "scripts": {
    "dev": "vite --host 127.0.0.1",
    "tauri:dev": "tauri dev",
    "build": "tsc && vite build",
    "typecheck": "tsc --noEmit",
    "test": "vitest",
    "test:run": "vitest run",
    "rust:check": "cd src-tauri && cargo check",
    "rust:test": "cd src-tauri && cargo test"
  },
  "dependencies": {
    "@tauri-apps/api": "^2.0.0",
    "@tanstack/react-query": "^5.51.1",
    "clsx": "^2.1.1",
    "lucide-react": "^0.468.0",
    "react": "^18.3.1",
    "react-dom": "^18.3.1"
  },
  "devDependencies": {
    "@tauri-apps/cli": "^2.0.0",
    "@testing-library/jest-dom": "^6.4.8",
    "@testing-library/react": "^16.0.1",
    "@testing-library/user-event": "^14.5.2",
    "@types/node": "^22.5.4",
    "@types/react": "^18.3.5",
    "@types/react-dom": "^18.3.0",
    "@vitejs/plugin-react": "^4.3.1",
    "autoprefixer": "^10.4.20",
    "jsdom": "^25.0.1",
    "postcss": "^8.4.41",
    "tailwindcss": "^3.4.10",
    "typescript": "^5.5.4",
    "vite": "^5.4.2",
    "vitest": "^2.0.5"
  }
}
```

Write `.gitignore`:

```gitignore
node_modules/
dist/
target/
src-tauri/target/
*.log
.env
.env.*
!.env.example
```

Write `LICENSE` using the MIT license text with copyright holder `xyito`.

Write `index.html`:

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>AI Switch</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

Write `tsconfig.json`:

```json
{
  "compilerOptions": {
    "target": "ES2020",
    "useDefineForClassFields": true,
    "lib": ["DOM", "DOM.Iterable", "ES2020"],
    "allowJs": false,
    "skipLibCheck": true,
    "esModuleInterop": true,
    "allowSyntheticDefaultImports": true,
    "strict": true,
    "forceConsistentCasingInFileNames": true,
    "module": "ESNext",
    "moduleResolution": "Node",
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true,
    "jsx": "react-jsx"
  },
  "include": ["src", "tests"],
  "references": [{ "path": "./tsconfig.node.json" }]
}
```

Write `tsconfig.node.json`:

```json
{
  "compilerOptions": {
    "composite": true,
    "module": "ESNext",
    "moduleResolution": "Node",
    "allowSyntheticDefaultImports": true
  },
  "include": ["vite.config.ts", "vitest.config.ts", "tailwind.config.ts"]
}
```

Write `vite.config.ts`:

```ts
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    host: "127.0.0.1",
    port: 1420,
    strictPort: true,
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: "es2020",
    minify: false,
  },
});
```

Write `vitest.config.ts`:

```ts
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [react()],
  test: {
    environment: "jsdom",
    setupFiles: ["src/test/setup.ts"],
    globals: true,
  },
});
```

Write `tailwind.config.ts`:

```ts
import type { Config } from "tailwindcss";

export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        ink: "#12110f",
        paper: "#f4efe5",
        moss: "#59684f",
        ember: "#c45b38",
        steel: "#516170",
      },
      fontFamily: {
        display: ["Aptos Display", "Bahnschrift", "Segoe UI", "sans-serif"],
        body: ["Aptos", "Segoe UI", "sans-serif"],
      },
    },
  },
  plugins: [],
} satisfies Config;
```

Write `postcss.config.cjs`:

```js
module.exports = {
  plugins: {
    tailwindcss: {},
    autoprefixer: {},
  },
};
```

- [ ] **Step 2: Create minimal React shell**

Write `src/main.tsx`:

```tsx
import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";
import "./styles.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
```

Write `src/App.tsx`:

```tsx
import { AppLayout } from "./components/layout/AppLayout";

export function App() {
  return (
    <AppLayout>
      <section className="rounded-3xl border border-ink/10 bg-white/70 p-8 shadow-xl shadow-ink/5">
        <p className="text-sm uppercase tracking-[0.3em] text-moss">Foundation</p>
        <h1 className="mt-3 font-display text-4xl font-semibold text-ink">AI Switch</h1>
        <p className="mt-4 max-w-2xl text-base leading-7 text-steel">
          Batch-first provider and official account switching foundation.
        </p>
      </section>
    </AppLayout>
  );
}
```

Write `src/components/layout/AppLayout.tsx`:

```tsx
type AppLayoutProps = {
  children: React.ReactNode;
};

export function AppLayout({ children }: AppLayoutProps) {
  return (
    <main className="min-h-screen bg-[radial-gradient(circle_at_top_left,#f8ddc8,transparent_32%),linear-gradient(135deg,#f4efe5,#dfe7df)] px-6 py-8 font-body text-ink">
      <div className="mx-auto max-w-7xl">{children}</div>
    </main>
  );
}
```

Write `src/components/ui/Button.tsx`:

```tsx
import type { ButtonHTMLAttributes } from "react";
import { clsx } from "clsx";

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "primary" | "secondary";
};

export function Button({ className, variant = "primary", ...props }: ButtonProps) {
  return (
    <button
      className={clsx(
        "rounded-full px-4 py-2 text-sm font-semibold transition",
        variant === "primary" && "bg-ink text-paper hover:bg-ink/90",
        variant === "secondary" && "border border-ink/15 bg-white/70 text-ink hover:bg-white",
        className,
      )}
      {...props}
    />
  );
}
```

Write `src/styles.css`:

```css
@tailwind base;
@tailwind components;
@tailwind utilities;

:root {
  color: #12110f;
  background: #f4efe5;
  font-synthesis: none;
  text-rendering: optimizeLegibility;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
}

body {
  margin: 0;
}
```

- [ ] **Step 3: Create Tauri shell**

Write `src-tauri/Cargo.toml`:

```toml
[package]
name = "ai-switch"
version = "0.1.0"
description = "AI provider and official account switcher"
authors = ["xyito"]
edition = "2021"

[lib]
name = "ai_switch_lib"
crate-type = ["staticlib", "cdylib", "rlib"]

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2", features = [] }
tauri-plugin-shell = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"
tokio = { version = "1", features = ["macros", "rt-multi-thread", "fs"] }
sqlx = { version = "0.8", features = ["runtime-tokio-rustls", "sqlite", "migrate", "chrono", "uuid", "json"] }
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
directories = "5"
keyring = "3"
sha2 = "0.10"
tempfile = "3"

[dev-dependencies]
assert_fs = "1"
```

Write `src-tauri/build.rs`:

```rust
fn main() {
    tauri_build::build();
}
```

Write `src-tauri/tauri.conf.json`:

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "AI Switch",
  "version": "0.1.0",
  "identifier": "io.xyito.ai-switch",
  "build": {
    "beforeDevCommand": "pnpm dev",
    "devUrl": "http://127.0.0.1:1420",
    "beforeBuildCommand": "pnpm build",
    "frontendDist": "../dist"
  },
  "app": {
    "windows": [
      {
        "title": "AI Switch",
        "width": 1180,
        "height": 760,
        "minWidth": 960,
        "minHeight": 640
      }
    ],
    "security": {
      "csp": null
    }
  },
  "bundle": {
    "active": false,
    "targets": "all"
  }
}
```

Write `src-tauri/src/main.rs`:

```rust
fn main() {
    ai_switch_lib::run();
}
```

Write `src-tauri/src/lib.rs`:

```rust
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .run(tauri::generate_context!())
        .expect("failed to run AI Switch");
}
```

- [ ] **Step 4: Install dependencies**

Run:

```powershell
corepack enable
pnpm install
```

Expected: dependencies install without errors and `pnpm-lock.yaml` is created.

- [ ] **Step 5: Verify scaffold**

Run:

```powershell
pnpm typecheck
pnpm test:run -- --passWithNoTests
pnpm rust:check
```

Expected: all commands exit with code `0`.

- [ ] **Step 6: Commit scaffold**

```powershell
git add .gitignore LICENSE index.html package.json pnpm-lock.yaml postcss.config.cjs tailwind.config.ts tsconfig.json tsconfig.node.json vite.config.ts vitest.config.ts src src-tauri
git commit -m "chore: scaffold tauri react app"
```

---

### Task 2: Add Backend Errors, Paths, Settings, And Tauri State

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Create: `src-tauri/src/error.rs`
- Create: `src-tauri/src/paths.rs`
- Create: `src-tauri/src/app_state.rs`
- Create: `src-tauri/src/models/mod.rs`
- Create: `src-tauri/src/models/settings.rs`
- Create: `src-tauri/src/services/mod.rs`
- Create: `src-tauri/src/services/settings_service.rs`
- Create: `src-tauri/src/commands/mod.rs`
- Create: `src-tauri/src/commands/settings_commands.rs`

**Interfaces:**
- Produces: `ApiError { code, message, details, recoverable, operation_id }`.
- Produces: `AppPaths::resolve() -> Result<AppPaths, AppError>`.
- Produces: `SettingsService::load(&AppPaths) -> Result<AppSettings, AppError>`.
- Produces: Tauri commands `get_settings` and `save_settings`.

- [ ] **Step 1: Write failing Rust tests for paths and settings**

Add these tests inside `src-tauri/src/services/settings_service.rs` after the implementation module is created in Step 3:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::AppPaths;
    use tempfile::tempdir;

    #[tokio::test]
    async fn load_creates_default_settings_when_file_is_missing() {
        let dir = tempdir().expect("tempdir");
        let paths = AppPaths::from_data_dir(dir.path().to_path_buf());

        let settings = SettingsService::load(&paths).await.expect("settings");

        assert_eq!(settings.language, "zh-CN");
        assert_eq!(settings.theme, "system");
        assert!(paths.settings_file.exists());
    }

    #[tokio::test]
    async fn save_then_load_round_trips_settings() {
        let dir = tempdir().expect("tempdir");
        let paths = AppPaths::from_data_dir(dir.path().to_path_buf());
        let settings = AppSettings {
            language: "en".to_string(),
            theme: "dark".to_string(),
            copy_import_sources: true,
            logging_enabled: true,
            secret_storage: "keyring".to_string(),
            data_dir: paths.data_dir.display().to_string(),
        };

        SettingsService::save(&paths, &settings).await.expect("save");
        let loaded = SettingsService::load(&paths).await.expect("load");

        assert_eq!(loaded.language, "en");
        assert_eq!(loaded.theme, "dark");
        assert!(loaded.copy_import_sources);
    }
}
```

- [ ] **Step 2: Run tests to verify failure**

Run:

```powershell
pnpm rust:test settings_service
```

Expected: FAIL because `SettingsService`, `AppPaths`, and `AppSettings` do not exist yet.

- [ ] **Step 3: Implement error, path, and settings modules**

Write `src-tauri/src/error.rs`:

```rust
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{message}")]
    Validation {
        code: &'static str,
        message: String,
        details: Option<String>,
        recoverable: bool,
    },
    #[error("{message}")]
    Filesystem {
        code: &'static str,
        message: String,
        details: Option<String>,
        recoverable: bool,
    },
    #[error("{message}")]
    Database {
        code: &'static str,
        message: String,
        details: Option<String>,
        recoverable: bool,
    },
    #[error("{message}")]
    Secret {
        code: &'static str,
        message: String,
        details: Option<String>,
        recoverable: bool,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    pub details: Option<String>,
    pub recoverable: bool,
    pub operation_id: Option<String>,
}

impl From<AppError> for ApiError {
    fn from(value: AppError) -> Self {
        match value {
            AppError::Validation { code, message, details, recoverable }
            | AppError::Filesystem { code, message, details, recoverable }
            | AppError::Database { code, message, details, recoverable }
            | AppError::Secret { code, message, details, recoverable } => Self {
                code: code.to_string(),
                message,
                details,
                recoverable,
                operation_id: None,
            },
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(value: std::io::Error) -> Self {
        AppError::Filesystem {
            code: "filesystem.io",
            message: "File operation failed".to_string(),
            details: Some(value.to_string()),
            recoverable: true,
        }
    }
}

impl From<serde_json::Error> for AppError {
    fn from(value: serde_json::Error) -> Self {
        AppError::Validation {
            code: "validation.json",
            message: "JSON data is invalid".to_string(),
            details: Some(value.to_string()),
            recoverable: true,
        }
    }
}
```

Write `src-tauri/src/paths.rs`:

```rust
use crate::error::AppError;
use directories::BaseDirs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub database_file: PathBuf,
    pub settings_file: PathBuf,
    pub backups_dir: PathBuf,
    pub imports_dir: PathBuf,
    pub logs_dir: PathBuf,
}

impl AppPaths {
    pub fn resolve() -> Result<Self, AppError> {
        let base = BaseDirs::new().ok_or_else(|| AppError::Filesystem {
            code: "filesystem.home_not_found",
            message: "Could not resolve the current user home directory".to_string(),
            details: None,
            recoverable: false,
        })?;
        Ok(Self::from_data_dir(base.home_dir().join(".ai-switch")))
    }

    pub fn from_data_dir(data_dir: PathBuf) -> Self {
        Self {
            database_file: data_dir.join("ai-switch.db"),
            settings_file: data_dir.join("settings.json"),
            backups_dir: data_dir.join("backups"),
            imports_dir: data_dir.join("imports"),
            logs_dir: data_dir.join("logs"),
            data_dir,
        }
    }

    pub async fn ensure(&self) -> Result<(), AppError> {
        tokio::fs::create_dir_all(&self.data_dir).await?;
        tokio::fs::create_dir_all(&self.backups_dir).await?;
        tokio::fs::create_dir_all(&self.imports_dir).await?;
        tokio::fs::create_dir_all(&self.logs_dir).await?;
        Ok(())
    }
}
```

Write `src-tauri/src/models/settings.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppSettings {
    pub language: String,
    pub theme: String,
    pub copy_import_sources: bool,
    pub logging_enabled: bool,
    pub secret_storage: String,
    pub data_dir: String,
}

impl AppSettings {
    pub fn defaults_for_data_dir(data_dir: String) -> Self {
        Self {
            language: "zh-CN".to_string(),
            theme: "system".to_string(),
            copy_import_sources: false,
            logging_enabled: true,
            secret_storage: "keyring".to_string(),
            data_dir,
        }
    }
}
```

Write `src-tauri/src/models/mod.rs`:

```rust
pub mod settings;
```

Write `src-tauri/src/services/settings_service.rs`:

```rust
use crate::error::AppError;
use crate::models::settings::AppSettings;
use crate::paths::AppPaths;

pub struct SettingsService;

impl SettingsService {
    pub async fn load(paths: &AppPaths) -> Result<AppSettings, AppError> {
        paths.ensure().await?;
        if !paths.settings_file.exists() {
            let settings = AppSettings::defaults_for_data_dir(paths.data_dir.display().to_string());
            Self::save(paths, &settings).await?;
            return Ok(settings);
        }

        let contents = tokio::fs::read_to_string(&paths.settings_file).await?;
        Ok(serde_json::from_str(&contents)?)
    }

    pub async fn save(paths: &AppPaths, settings: &AppSettings) -> Result<(), AppError> {
        paths.ensure().await?;
        let contents = serde_json::to_string_pretty(settings)?;
        tokio::fs::write(&paths.settings_file, contents).await?;
        Ok(())
    }
}
```

Write `src-tauri/src/services/mod.rs`:

```rust
pub mod settings_service;
```

Write `src-tauri/src/app_state.rs`:

```rust
use crate::paths::AppPaths;

#[derive(Debug, Clone)]
pub struct AppState {
    pub paths: AppPaths,
}
```

Write `src-tauri/src/commands/settings_commands.rs`:

```rust
use crate::app_state::AppState;
use crate::error::ApiError;
use crate::models::settings::AppSettings;
use crate::services::settings_service::SettingsService;
use tauri::State;

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, ApiError> {
    SettingsService::load(&state.paths).await.map_err(ApiError::from)
}

#[tauri::command]
pub async fn save_settings(
    state: State<'_, AppState>,
    settings: AppSettings,
) -> Result<AppSettings, ApiError> {
    SettingsService::save(&state.paths, &settings).await.map_err(ApiError::from)?;
    Ok(settings)
}
```

Write `src-tauri/src/commands/mod.rs`:

```rust
pub mod settings_commands;
```

Update `src-tauri/src/lib.rs`:

```rust
mod app_state;
mod commands;
mod error;
mod models;
mod paths;
mod services;

use app_state::AppState;
use commands::settings_commands::{get_settings, save_settings};
use paths::AppPaths;

pub fn run() {
    let paths = AppPaths::resolve().expect("failed to resolve app paths");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState { paths })
        .invoke_handler(tauri::generate_handler![get_settings, save_settings])
        .run(tauri::generate_context!())
        .expect("failed to run AI Switch");
}
```

- [ ] **Step 4: Run tests to verify pass**

Run:

```powershell
pnpm rust:test settings_service
pnpm rust:check
```

Expected: PASS and `cargo check` exits with code `0`.

- [ ] **Step 5: Commit settings foundation**

```powershell
git add src-tauri/src
git commit -m "feat: add settings and app state foundation"
```

---

### Task 3: Add SQLite Migrations, Database Pool, Models, And Repository Tests

**Files:**
- Create: `src-tauri/migrations/202607130001_foundation.sql`
- Create: `src-tauri/src/database/mod.rs`
- Create: `src-tauri/src/database/test_support.rs`
- Create: `src-tauri/src/models/batch.rs`
- Create: `src-tauri/src/models/provider.rs`
- Create: `src-tauri/src/models/account.rs`
- Create: `src-tauri/src/models/import_job.rs`
- Create: `src-tauri/src/models/target_app.rs`
- Modify: `src-tauri/src/models/mod.rs`
- Modify: `src-tauri/src/app_state.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Produces: `create_pool(database_file: &Path) -> Result<SqlitePool, AppError>`.
- Produces: `run_migrations(pool: &SqlitePool) -> Result<(), AppError>`.
- Produces model structs `Batch`, `Provider`, `OfficialAccount`, `ImportJob`, `TargetApp`, `BatchItem`, and `BatchGroup`.

- [ ] **Step 1: Write migration file**

Write `src-tauri/migrations/202607130001_foundation.sql`:

```sql
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS target_apps (
  id TEXT PRIMARY KEY,
  key TEXT NOT NULL UNIQUE,
  display_name TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1,
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS providers (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  kind TEXT NOT NULL,
  base_url TEXT,
  model_config_json TEXT NOT NULL DEFAULT '{}',
  target_options_json TEXT NOT NULL DEFAULT '{}',
  secret_ref TEXT,
  status TEXT NOT NULL DEFAULT 'ok',
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS official_accounts (
  id TEXT PRIMARY KEY,
  platform TEXT NOT NULL,
  display_name TEXT NOT NULL,
  email TEXT,
  plan TEXT,
  account_metadata_json TEXT NOT NULL DEFAULT '{}',
  secret_ref TEXT,
  quota_snapshot_id TEXT,
  status TEXT NOT NULL DEFAULT 'ok',
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS batches (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  source TEXT NOT NULL DEFAULT 'manual',
  notes TEXT,
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS batch_items (
  id TEXT PRIMARY KEY,
  batch_id TEXT NOT NULL,
  item_type TEXT NOT NULL CHECK (item_type IN ('provider', 'official_account')),
  item_id TEXT NOT NULL,
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  UNIQUE(batch_id, item_type, item_id),
  FOREIGN KEY(batch_id) REFERENCES batches(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS import_jobs (
  id TEXT PRIMARY KEY,
  source_type TEXT NOT NULL,
  source_label TEXT NOT NULL,
  batch_id TEXT,
  strategy TEXT NOT NULL,
  status TEXT NOT NULL,
  success_count INTEGER NOT NULL DEFAULT 0,
  failure_count INTEGER NOT NULL DEFAULT 0,
  conflict_count INTEGER NOT NULL DEFAULT 0,
  summary_json TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL,
  completed_at TEXT,
  FOREIGN KEY(batch_id) REFERENCES batches(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS target_app_states (
  id TEXT PRIMARY KEY,
  target_app_id TEXT NOT NULL UNIQUE,
  active_item_type TEXT,
  active_item_id TEXT,
  last_write_status TEXT,
  last_error_code TEXT,
  last_written_at TEXT,
  updated_at TEXT NOT NULL,
  FOREIGN KEY(target_app_id) REFERENCES target_apps(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS config_snapshots (
  id TEXT PRIMARY KEY,
  target_app_id TEXT,
  operation TEXT NOT NULL,
  path TEXT NOT NULL,
  before_hash TEXT,
  after_hash TEXT,
  backup_path TEXT,
  status TEXT NOT NULL,
  error_code TEXT,
  created_at TEXT NOT NULL,
  FOREIGN KEY(target_app_id) REFERENCES target_apps(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS quota_snapshots (
  id TEXT PRIMARY KEY,
  owner_type TEXT NOT NULL,
  owner_id TEXT NOT NULL,
  status TEXT NOT NULL,
  remaining_label TEXT,
  reset_at TEXT,
  summary_json TEXT NOT NULL DEFAULT '{}',
  raw_excerpt_json TEXT NOT NULL DEFAULT '{}',
  fetched_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS secure_secrets (
  id TEXT PRIMARY KEY,
  provider TEXT NOT NULL,
  external_ref TEXT NOT NULL,
  label TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_batch_items_batch_id ON batch_items(batch_id);
CREATE INDEX IF NOT EXISTS idx_batch_items_item ON batch_items(item_type, item_id);
CREATE INDEX IF NOT EXISTS idx_providers_name ON providers(name);
CREATE INDEX IF NOT EXISTS idx_accounts_platform ON official_accounts(platform);
CREATE INDEX IF NOT EXISTS idx_import_jobs_created_at ON import_jobs(created_at);
```

- [ ] **Step 2: Write database test before implementation**

Write `src-tauri/src/database/test_support.rs`:

```rust
use super::{create_memory_pool, run_migrations};
use sqlx::Row;

#[tokio::test]
async fn migrations_create_foundation_tables() {
    let pool = create_memory_pool().await.expect("pool");
    run_migrations(&pool).await.expect("migrations");

    let row = sqlx::query("SELECT COUNT(*) as count FROM sqlite_master WHERE type = 'table' AND name IN ('target_apps', 'providers', 'official_accounts', 'batches', 'batch_items', 'import_jobs')")
        .fetch_one(&pool)
        .await
        .expect("table count");

    let count: i64 = row.get("count");
    assert_eq!(count, 6);
}
```

- [ ] **Step 3: Run database test to verify failure**

Run:

```powershell
pnpm rust:test migrations_create_foundation_tables
```

Expected: FAIL because database helpers are not implemented.

- [ ] **Step 4: Implement database module**

Write `src-tauri/src/database/mod.rs`:

```rust
use crate::error::AppError;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::path::Path;
use std::str::FromStr;

pub mod repositories;

#[cfg(test)]
mod test_support;

pub async fn create_pool(database_file: &Path) -> Result<SqlitePool, AppError> {
    if let Some(parent) = database_file.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let url = format!("sqlite://{}", database_file.display());
    let options = SqliteConnectOptions::from_str(&url)
        .map_err(|err| AppError::Database {
            code: "database.connect_options",
            message: "Could not create SQLite connection options".to_string(),
            details: Some(err.to_string()),
            recoverable: false,
        })?
        .create_if_missing(true)
        .foreign_keys(true);

    SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .map_err(|err| AppError::Database {
            code: "database.connect",
            message: "Could not connect to SQLite database".to_string(),
            details: Some(err.to_string()),
            recoverable: false,
        })
}

pub async fn create_memory_pool() -> Result<SqlitePool, AppError> {
    let options = SqliteConnectOptions::from_str("sqlite::memory:")
        .map_err(|err| AppError::Database {
            code: "database.connect_options",
            message: "Could not create in-memory SQLite options".to_string(),
            details: Some(err.to_string()),
            recoverable: false,
        })?
        .foreign_keys(true);

    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|err| AppError::Database {
            code: "database.connect",
            message: "Could not connect to in-memory SQLite database".to_string(),
            details: Some(err.to_string()),
            recoverable: false,
        })
}

pub async fn run_migrations(pool: &SqlitePool) -> Result<(), AppError> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.migration",
            message: "Could not apply SQLite migrations".to_string(),
            details: Some(err.to_string()),
            recoverable: false,
        })
}
```

Create empty repository module for compilation in `src-tauri/src/database/repositories/mod.rs`:

```rust
pub mod account_repository;
pub mod batch_repository;
pub mod import_repository;
pub mod provider_repository;
pub mod target_repository;
```

Create module files with this compile-safe content:

```rust
//! Repository module participating in the database module graph.
```

Use that exact content for:

- `src-tauri/src/database/repositories/account_repository.rs`
- `src-tauri/src/database/repositories/batch_repository.rs`
- `src-tauri/src/database/repositories/import_repository.rs`
- `src-tauri/src/database/repositories/provider_repository.rs`
- `src-tauri/src/database/repositories/target_repository.rs`

- [ ] **Step 5: Add model structs**

Write `src-tauri/src/models/batch.rs`:

```rust
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct Batch {
    pub id: String,
    pub name: String,
    pub source: String,
    pub notes: Option<String>,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct BatchItem {
    pub id: String,
    pub batch_id: String,
    pub item_type: String,
    pub item_id: String,
    pub sort_order: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BatchChild {
    pub item_type: String,
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BatchGroup {
    pub batch: Batch,
    pub health: String,
    pub children: Vec<BatchChild>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewBatch {
    pub name: String,
    pub source: String,
    pub notes: Option<String>,
}
```

Write `src-tauri/src/models/provider.rs`:

```rust
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub base_url: Option<String>,
    pub model_config_json: String,
    pub target_options_json: String,
    pub secret_ref: Option<String>,
    pub status: String,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewProvider {
    pub name: String,
    pub kind: String,
    pub base_url: Option<String>,
    pub model_config_json: String,
    pub target_options_json: String,
    pub secret_ref: Option<String>,
}
```

Write `src-tauri/src/models/account.rs`:

```rust
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct OfficialAccount {
    pub id: String,
    pub platform: String,
    pub display_name: String,
    pub email: Option<String>,
    pub plan: Option<String>,
    pub account_metadata_json: String,
    pub secret_ref: Option<String>,
    pub quota_snapshot_id: Option<String>,
    pub status: String,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewOfficialAccount {
    pub platform: String,
    pub display_name: String,
    pub email: Option<String>,
    pub plan: Option<String>,
    pub account_metadata_json: String,
    pub secret_ref: Option<String>,
}
```

Write `src-tauri/src/models/import_job.rs`:

```rust
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct ImportJob {
    pub id: String,
    pub source_type: String,
    pub source_label: String,
    pub batch_id: Option<String>,
    pub strategy: String,
    pub status: String,
    pub success_count: i64,
    pub failure_count: i64,
    pub conflict_count: i64,
    pub summary_json: String,
    pub created_at: String,
    pub completed_at: Option<String>,
}
```

Write `src-tauri/src/models/target_app.rs`:

```rust
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct TargetApp {
    pub id: String,
    pub key: String,
    pub display_name: String,
    pub enabled: i64,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}
```

Update `src-tauri/src/models/mod.rs`:

```rust
pub mod account;
pub mod batch;
pub mod import_job;
pub mod provider;
pub mod settings;
pub mod target_app;
```

- [ ] **Step 6: Wire database into app state**

Update `src-tauri/src/app_state.rs`:

```rust
use crate::paths::AppPaths;
use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct AppState {
    pub paths: AppPaths,
    pub pool: SqlitePool,
}
```

Update `src-tauri/src/lib.rs`:

```rust
mod app_state;
mod commands;
mod database;
mod error;
mod models;
mod paths;
mod services;

use app_state::AppState;
use commands::settings_commands::{get_settings, save_settings};
use database::{create_pool, run_migrations};
use paths::AppPaths;

pub fn run() {
    let paths = AppPaths::resolve().expect("failed to resolve app paths");
    let pool = tauri::async_runtime::block_on(async {
        paths.ensure().await.expect("failed to ensure app paths");
        let pool = create_pool(&paths.database_file).await.expect("failed to create database pool");
        run_migrations(&pool).await.expect("failed to run database migrations");
        pool
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState { paths, pool })
        .invoke_handler(tauri::generate_handler![get_settings, save_settings])
        .run(tauri::generate_context!())
        .expect("failed to run AI Switch");
}
```

- [ ] **Step 7: Run tests and check**

Run:

```powershell
pnpm rust:test migrations_create_foundation_tables
pnpm rust:check
```

Expected: PASS and `cargo check` exits with code `0`.

- [ ] **Step 8: Commit database foundation**

```powershell
git add src-tauri/migrations src-tauri/src
git commit -m "feat: add sqlite foundation schema"
```

---

### Task 4: Implement Repositories And Batch-First Grouping

**Files:**
- Modify: `src-tauri/src/database/repositories/batch_repository.rs`
- Modify: `src-tauri/src/database/repositories/provider_repository.rs`
- Modify: `src-tauri/src/database/repositories/account_repository.rs`
- Modify: `src-tauri/src/database/repositories/import_repository.rs`
- Modify: `src-tauri/src/database/repositories/target_repository.rs`

**Interfaces:**
- Produces: `BatchRepository::create(pool, NewBatch) -> Result<Batch, AppError>`.
- Produces: `BatchRepository::add_item(pool, batch_id, item_type, item_id) -> Result<BatchItem, AppError>`.
- Produces: `BatchRepository::list_groups(pool, search) -> Result<Vec<BatchGroup>, AppError>`.
- Produces: `ProviderRepository::create(pool, NewProvider) -> Result<Provider, AppError>`.
- Produces: `AccountRepository::create(pool, NewOfficialAccount) -> Result<OfficialAccount, AppError>`.
- Produces: `ImportRepository::list_recent(pool) -> Result<Vec<ImportJob>, AppError>`.
- Produces: `TargetRepository::ensure_defaults(pool) -> Result<Vec<TargetApp>, AppError>`.

- [ ] **Step 1: Write repository tests**

Append tests to `src-tauri/src/database/repositories/batch_repository.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{create_memory_pool, run_migrations};
    use crate::database::repositories::account_repository::AccountRepository;
    use crate::database::repositories::provider_repository::ProviderRepository;
    use crate::models::account::NewOfficialAccount;
    use crate::models::batch::NewBatch;
    use crate::models::provider::NewProvider;

    #[tokio::test]
    async fn list_groups_returns_batch_with_provider_and_account_children() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let batch = BatchRepository::create(&pool, NewBatch {
            name: "July imports".to_string(),
            source: "example_json".to_string(),
            notes: None,
        }).await.expect("batch");

        let provider = ProviderRepository::create(&pool, NewProvider {
            name: "Acme Claude".to_string(),
            kind: "openai_compatible".to_string(),
            base_url: Some("https://api.example.com/v1".to_string()),
            model_config_json: "{}".to_string(),
            target_options_json: "{}".to_string(),
            secret_ref: Some("secret://provider/acme".to_string()),
        }).await.expect("provider");

        let account = AccountRepository::create(&pool, NewOfficialAccount {
            platform: "codex".to_string(),
            display_name: "Team Account".to_string(),
            email: Some("team@example.com".to_string()),
            plan: Some("team".to_string()),
            account_metadata_json: "{}".to_string(),
            secret_ref: Some("secret://account/team".to_string()),
        }).await.expect("account");

        BatchRepository::add_item(&pool, &batch.id, "provider", &provider.id).await.expect("provider link");
        BatchRepository::add_item(&pool, &batch.id, "official_account", &account.id).await.expect("account link");

        let groups = BatchRepository::list_groups(&pool, None).await.expect("groups");

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].batch.name, "July imports");
        assert_eq!(groups[0].health, "ok");
        assert_eq!(groups[0].children.len(), 2);
    }
}
```

- [ ] **Step 2: Run repository test to verify failure**

Run:

```powershell
pnpm rust:test list_groups_returns_batch_with_provider_and_account_children
```

Expected: FAIL because repository methods are not implemented.

- [ ] **Step 3: Implement provider and account repositories**

Write `src-tauri/src/database/repositories/provider_repository.rs`:

```rust
use crate::error::AppError;
use crate::models::provider::{NewProvider, Provider};
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct ProviderRepository;

impl ProviderRepository {
    pub async fn create(pool: &SqlitePool, input: NewProvider) -> Result<Provider, AppError> {
        let now = Utc::now().to_rfc3339();
        let id = Uuid::new_v4().to_string();

        sqlx::query(
            "INSERT INTO providers (id, name, kind, base_url, model_config_json, target_options_json, secret_ref, status, sort_order, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, 'ok', 0, ?, ?)"
        )
        .bind(&id)
        .bind(&input.name)
        .bind(&input.kind)
        .bind(&input.base_url)
        .bind(&input.model_config_json)
        .bind(&input.target_options_json)
        .bind(&input.secret_ref)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.provider_create",
            message: "Could not create provider".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Self::get(pool, &id).await
    }

    pub async fn get(pool: &SqlitePool, id: &str) -> Result<Provider, AppError> {
        sqlx::query_as::<_, Provider>("SELECT * FROM providers WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.provider_get",
                message: "Could not load provider".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }
}
```

Write `src-tauri/src/database/repositories/account_repository.rs`:

```rust
use crate::error::AppError;
use crate::models::account::{NewOfficialAccount, OfficialAccount};
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct AccountRepository;

impl AccountRepository {
    pub async fn create(pool: &SqlitePool, input: NewOfficialAccount) -> Result<OfficialAccount, AppError> {
        let now = Utc::now().to_rfc3339();
        let id = Uuid::new_v4().to_string();

        sqlx::query(
            "INSERT INTO official_accounts (id, platform, display_name, email, plan, account_metadata_json, secret_ref, status, sort_order, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, 'ok', 0, ?, ?)"
        )
        .bind(&id)
        .bind(&input.platform)
        .bind(&input.display_name)
        .bind(&input.email)
        .bind(&input.plan)
        .bind(&input.account_metadata_json)
        .bind(&input.secret_ref)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.account_create",
            message: "Could not create official account".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Self::get(pool, &id).await
    }

    pub async fn get(pool: &SqlitePool, id: &str) -> Result<OfficialAccount, AppError> {
        sqlx::query_as::<_, OfficialAccount>("SELECT * FROM official_accounts WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.account_get",
                message: "Could not load official account".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }
}
```

- [ ] **Step 4: Implement batch repository**

Write `src-tauri/src/database/repositories/batch_repository.rs` with the test from Step 1 kept at the end:

```rust
use crate::error::AppError;
use crate::models::batch::{Batch, BatchChild, BatchGroup, BatchItem, NewBatch};
use chrono::Utc;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

pub struct BatchRepository;

impl BatchRepository {
    pub async fn create(pool: &SqlitePool, input: NewBatch) -> Result<Batch, AppError> {
        let now = Utc::now().to_rfc3339();
        let id = Uuid::new_v4().to_string();

        sqlx::query("INSERT INTO batches (id, name, source, notes, sort_order, created_at, updated_at) VALUES (?, ?, ?, ?, 0, ?, ?)")
            .bind(&id)
            .bind(&input.name)
            .bind(&input.source)
            .bind(&input.notes)
            .bind(&now)
            .bind(&now)
            .execute(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.batch_create",
                message: "Could not create batch".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;

        Self::get(pool, &id).await
    }

    pub async fn get(pool: &SqlitePool, id: &str) -> Result<Batch, AppError> {
        sqlx::query_as::<_, Batch>("SELECT * FROM batches WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.batch_get",
                message: "Could not load batch".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }

    pub async fn add_item(pool: &SqlitePool, batch_id: &str, item_type: &str, item_id: &str) -> Result<BatchItem, AppError> {
        if item_type != "provider" && item_type != "official_account" {
            return Err(AppError::Validation {
                code: "validation.batch_item_type",
                message: "Batch item type must be provider or official_account".to_string(),
                details: Some(item_type.to_string()),
                recoverable: true,
            });
        }

        let now = Utc::now().to_rfc3339();
        let id = Uuid::new_v4().to_string();

        sqlx::query("INSERT INTO batch_items (id, batch_id, item_type, item_id, sort_order, created_at) VALUES (?, ?, ?, ?, 0, ?)")
            .bind(&id)
            .bind(batch_id)
            .bind(item_type)
            .bind(item_id)
            .bind(&now)
            .execute(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.batch_item_create",
                message: "Could not attach item to batch".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;

        sqlx::query_as::<_, BatchItem>("SELECT * FROM batch_items WHERE id = ?")
            .bind(&id)
            .fetch_one(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.batch_item_get",
                message: "Could not load batch item".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }

    pub async fn list_groups(pool: &SqlitePool, search: Option<&str>) -> Result<Vec<BatchGroup>, AppError> {
        let batches = sqlx::query_as::<_, Batch>("SELECT * FROM batches ORDER BY sort_order ASC, created_at DESC")
            .fetch_all(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.batch_list",
                message: "Could not list batches".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;

        let mut groups = Vec::new();
        let needle = search.map(|value| value.to_lowercase());

        for batch in batches {
            let children = Self::children_for_batch(pool, &batch.id).await?;
            let filtered_children: Vec<BatchChild> = match &needle {
                Some(value) => children
                    .into_iter()
                    .filter(|child| {
                        batch.name.to_lowercase().contains(value)
                            || child.title.to_lowercase().contains(value)
                            || child.subtitle.clone().unwrap_or_default().to_lowercase().contains(value)
                    })
                    .collect(),
                None => children,
            };

            if needle.is_none() || batch.name.to_lowercase().contains(needle.as_ref().unwrap()) || !filtered_children.is_empty() {
                let health = if filtered_children.iter().any(|child| child.status == "error") {
                    "error"
                } else if filtered_children.iter().any(|child| child.status == "warning") {
                    "warning"
                } else {
                    "ok"
                };
                groups.push(BatchGroup {
                    batch,
                    health: health.to_string(),
                    children: filtered_children,
                });
            }
        }

        Ok(groups)
    }

    async fn children_for_batch(pool: &SqlitePool, batch_id: &str) -> Result<Vec<BatchChild>, AppError> {
        let rows = sqlx::query(
            "SELECT bi.item_type, bi.item_id, p.name as provider_name, p.kind as provider_kind, p.status as provider_status,
                    a.display_name as account_name, a.platform as account_platform, a.email as account_email, a.status as account_status
             FROM batch_items bi
             LEFT JOIN providers p ON bi.item_type = 'provider' AND bi.item_id = p.id
             LEFT JOIN official_accounts a ON bi.item_type = 'official_account' AND bi.item_id = a.id
             WHERE bi.batch_id = ?
             ORDER BY bi.sort_order ASC, bi.created_at ASC"
        )
        .bind(batch_id)
        .fetch_all(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.batch_children",
            message: "Could not load batch children".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;

        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let item_type: String = row.get("item_type");
                let id: String = row.get("item_id");
                if item_type == "provider" {
                    Some(BatchChild {
                        item_type,
                        id,
                        title: row.get::<Option<String>, _>("provider_name").unwrap_or_default(),
                        subtitle: row.get::<Option<String>, _>("provider_kind"),
                        status: row.get::<Option<String>, _>("provider_status").unwrap_or_else(|| "error".to_string()),
                    })
                } else {
                    let email: Option<String> = row.get("account_email");
                    Some(BatchChild {
                        item_type,
                        id,
                        title: row.get::<Option<String>, _>("account_name").unwrap_or_default(),
                        subtitle: email.or_else(|| row.get::<Option<String>, _>("account_platform")),
                        status: row.get::<Option<String>, _>("account_status").unwrap_or_else(|| "error".to_string()),
                    })
                }
            })
            .collect())
    }
}
```

- [ ] **Step 5: Implement import and target repositories**

Write `src-tauri/src/database/repositories/import_repository.rs`:

```rust
use crate::error::AppError;
use crate::models::import_job::ImportJob;
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct ImportRepository;

impl ImportRepository {
    pub async fn create_job(pool: &SqlitePool, source_type: &str, source_label: &str, batch_id: Option<&str>, strategy: &str) -> Result<ImportJob, AppError> {
        let now = Utc::now().to_rfc3339();
        let id = Uuid::new_v4().to_string();

        sqlx::query("INSERT INTO import_jobs (id, source_type, source_label, batch_id, strategy, status, summary_json, created_at) VALUES (?, ?, ?, ?, ?, 'running', '{}', ?)")
            .bind(&id)
            .bind(source_type)
            .bind(source_label)
            .bind(batch_id)
            .bind(strategy)
            .bind(&now)
            .execute(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.import_job_create",
                message: "Could not create import job".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;

        Self::get(pool, &id).await
    }

    pub async fn complete_job(pool: &SqlitePool, id: &str, status: &str, success_count: i64, failure_count: i64, conflict_count: i64, summary_json: &str) -> Result<ImportJob, AppError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query("UPDATE import_jobs SET status = ?, success_count = ?, failure_count = ?, conflict_count = ?, summary_json = ?, completed_at = ? WHERE id = ?")
            .bind(status)
            .bind(success_count)
            .bind(failure_count)
            .bind(conflict_count)
            .bind(summary_json)
            .bind(&now)
            .bind(id)
            .execute(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.import_job_complete",
                message: "Could not complete import job".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;

        Self::get(pool, id).await
    }

    pub async fn get(pool: &SqlitePool, id: &str) -> Result<ImportJob, AppError> {
        sqlx::query_as::<_, ImportJob>("SELECT * FROM import_jobs WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.import_job_get",
                message: "Could not load import job".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }

    pub async fn list_recent(pool: &SqlitePool) -> Result<Vec<ImportJob>, AppError> {
        sqlx::query_as::<_, ImportJob>("SELECT * FROM import_jobs ORDER BY created_at DESC LIMIT 50")
            .fetch_all(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.import_job_list",
                message: "Could not list import jobs".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }
}
```

Write `src-tauri/src/database/repositories/target_repository.rs`:

```rust
use crate::error::AppError;
use crate::models::target_app::TargetApp;
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct TargetRepository;

impl TargetRepository {
    pub async fn ensure_defaults(pool: &SqlitePool) -> Result<Vec<TargetApp>, AppError> {
        let defaults = [
            ("claude_code", "Claude Code"),
            ("claude_desktop", "Claude Desktop"),
            ("codex", "Codex"),
            ("gemini_cli", "Gemini CLI"),
            ("opencode", "OpenCode"),
            ("openclaw", "OpenClaw"),
            ("hermes", "Hermes"),
        ];

        for (index, (key, display_name)) in defaults.iter().enumerate() {
            let exists: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM target_apps WHERE key = ?")
                .bind(key)
                .fetch_one(pool)
                .await
                .map_err(|err| AppError::Database {
                    code: "database.target_count",
                    message: "Could not count target apps".to_string(),
                    details: Some(err.to_string()),
                    recoverable: true,
                })?;

            if exists.0 == 0 {
                let now = Utc::now().to_rfc3339();
                sqlx::query("INSERT INTO target_apps (id, key, display_name, enabled, sort_order, created_at, updated_at) VALUES (?, ?, ?, 1, ?, ?, ?)")
                    .bind(Uuid::new_v4().to_string())
                    .bind(key)
                    .bind(display_name)
                    .bind(index as i64)
                    .bind(&now)
                    .bind(&now)
                    .execute(pool)
                    .await
                    .map_err(|err| AppError::Database {
                        code: "database.target_insert",
                        message: "Could not insert target app".to_string(),
                        details: Some(err.to_string()),
                        recoverable: true,
                    })?;
            }
        }

        sqlx::query_as::<_, TargetApp>("SELECT * FROM target_apps ORDER BY sort_order ASC")
            .fetch_all(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.target_list",
                message: "Could not list target apps".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }
}
```

- [ ] **Step 6: Run repository tests**

Run:

```powershell
pnpm rust:test list_groups_returns_batch_with_provider_and_account_children
pnpm rust:test migrations_create_foundation_tables
pnpm rust:check
```

Expected: PASS and `cargo check` exits with code `0`.

- [ ] **Step 7: Commit repositories**

```powershell
git add src-tauri/src/database src-tauri/src/models
git commit -m "feat: add repositories and batch grouping"
```

---

### Task 5: Add Services And Tauri Commands For Core Data

**Files:**
- Create: `src-tauri/src/services/batch_service.rs`
- Create: `src-tauri/src/services/target_service.rs`
- Modify: `src-tauri/src/services/mod.rs`
- Create: `src-tauri/src/commands/batch_commands.rs`
- Create: `src-tauri/src/commands/target_commands.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Produces: `BatchService::create_batch(pool, NewBatch)`.
- Produces: `BatchService::list_groups(pool, Option<String>)`.
- Produces: Tauri commands `create_batch`, `list_batch_groups`, `create_provider`, `create_official_account`, `list_target_apps`.

- [ ] **Step 1: Write service tests**

Add tests in `src-tauri/src/services/batch_service.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{create_memory_pool, run_migrations};
    use crate::models::batch::NewBatch;

    #[tokio::test]
    async fn create_batch_rejects_empty_name() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let result = BatchService::create_batch(&pool, NewBatch {
            name: " ".to_string(),
            source: "manual".to_string(),
            notes: None,
        }).await;

        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Run service test to verify failure**

Run:

```powershell
pnpm rust:test create_batch_rejects_empty_name
```

Expected: FAIL because `BatchService` is not implemented.

- [ ] **Step 3: Implement services**

Write `src-tauri/src/services/batch_service.rs`:

```rust
use crate::database::repositories::account_repository::AccountRepository;
use crate::database::repositories::batch_repository::BatchRepository;
use crate::database::repositories::provider_repository::ProviderRepository;
use crate::error::AppError;
use crate::models::account::{NewOfficialAccount, OfficialAccount};
use crate::models::batch::{Batch, BatchGroup, NewBatch};
use crate::models::provider::{NewProvider, Provider};
use sqlx::SqlitePool;

pub struct BatchService;

impl BatchService {
    pub async fn create_batch(pool: &SqlitePool, input: NewBatch) -> Result<Batch, AppError> {
        if input.name.trim().is_empty() {
            return Err(AppError::Validation {
                code: "validation.batch_name_required",
                message: "Batch name is required".to_string(),
                details: None,
                recoverable: true,
            });
        }

        BatchRepository::create(pool, NewBatch {
            name: input.name.trim().to_string(),
            source: input.source,
            notes: input.notes,
        }).await
    }

    pub async fn create_provider(pool: &SqlitePool, input: NewProvider, batch_id: Option<String>) -> Result<Provider, AppError> {
        if input.name.trim().is_empty() {
            return Err(AppError::Validation {
                code: "validation.provider_name_required",
                message: "Provider name is required".to_string(),
                details: None,
                recoverable: true,
            });
        }

        let provider = ProviderRepository::create(pool, input).await?;
        if let Some(batch_id) = batch_id {
            BatchRepository::add_item(pool, &batch_id, "provider", &provider.id).await?;
        }
        Ok(provider)
    }

    pub async fn create_official_account(pool: &SqlitePool, input: NewOfficialAccount, batch_id: Option<String>) -> Result<OfficialAccount, AppError> {
        if input.display_name.trim().is_empty() {
            return Err(AppError::Validation {
                code: "validation.account_name_required",
                message: "Account display name is required".to_string(),
                details: None,
                recoverable: true,
            });
        }

        let account = AccountRepository::create(pool, input).await?;
        if let Some(batch_id) = batch_id {
            BatchRepository::add_item(pool, &batch_id, "official_account", &account.id).await?;
        }
        Ok(account)
    }

    pub async fn list_groups(pool: &SqlitePool, search: Option<String>) -> Result<Vec<BatchGroup>, AppError> {
        BatchRepository::list_groups(pool, search.as_deref()).await
    }
}
```

Write `src-tauri/src/services/target_service.rs`:

```rust
use crate::database::repositories::target_repository::TargetRepository;
use crate::error::AppError;
use crate::models::target_app::TargetApp;
use sqlx::SqlitePool;

pub struct TargetService;

impl TargetService {
    pub async fn list_targets(pool: &SqlitePool) -> Result<Vec<TargetApp>, AppError> {
        TargetRepository::ensure_defaults(pool).await
    }
}
```

Update `src-tauri/src/services/mod.rs`:

```rust
pub mod batch_service;
pub mod settings_service;
pub mod target_service;
```

- [ ] **Step 4: Implement Tauri commands**

Write `src-tauri/src/commands/batch_commands.rs`:

```rust
use crate::app_state::AppState;
use crate::error::ApiError;
use crate::models::account::{NewOfficialAccount, OfficialAccount};
use crate::models::batch::{Batch, BatchGroup, NewBatch};
use crate::models::provider::{NewProvider, Provider};
use crate::services::batch_service::BatchService;
use serde::Deserialize;
use tauri::State;

#[derive(Debug, Deserialize)]
pub struct CreateProviderRequest {
    pub provider: NewProvider,
    pub batch_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateAccountRequest {
    pub account: NewOfficialAccount,
    pub batch_id: Option<String>,
}

#[tauri::command]
pub async fn create_batch(state: State<'_, AppState>, input: NewBatch) -> Result<Batch, ApiError> {
    BatchService::create_batch(&state.pool, input).await.map_err(ApiError::from)
}

#[tauri::command]
pub async fn list_batch_groups(state: State<'_, AppState>, search: Option<String>) -> Result<Vec<BatchGroup>, ApiError> {
    BatchService::list_groups(&state.pool, search).await.map_err(ApiError::from)
}

#[tauri::command]
pub async fn create_provider(state: State<'_, AppState>, request: CreateProviderRequest) -> Result<Provider, ApiError> {
    BatchService::create_provider(&state.pool, request.provider, request.batch_id).await.map_err(ApiError::from)
}

#[tauri::command]
pub async fn create_official_account(state: State<'_, AppState>, request: CreateAccountRequest) -> Result<OfficialAccount, ApiError> {
    BatchService::create_official_account(&state.pool, request.account, request.batch_id).await.map_err(ApiError::from)
}
```

Write `src-tauri/src/commands/target_commands.rs`:

```rust
use crate::app_state::AppState;
use crate::error::ApiError;
use crate::models::target_app::TargetApp;
use crate::services::target_service::TargetService;
use tauri::State;

#[tauri::command]
pub async fn list_target_apps(state: State<'_, AppState>) -> Result<Vec<TargetApp>, ApiError> {
    TargetService::list_targets(&state.pool).await.map_err(ApiError::from)
}
```

Update `src-tauri/src/commands/mod.rs`:

```rust
pub mod batch_commands;
pub mod settings_commands;
pub mod target_commands;
```

Update command imports and handler in `src-tauri/src/lib.rs`:

```rust
use commands::batch_commands::{create_batch, create_official_account, create_provider, list_batch_groups};
use commands::settings_commands::{get_settings, save_settings};
use commands::target_commands::list_target_apps;
```

Use this handler:

```rust
.invoke_handler(tauri::generate_handler![
    get_settings,
    save_settings,
    create_batch,
    list_batch_groups,
    create_provider,
    create_official_account,
    list_target_apps
])
```

- [ ] **Step 5: Run service and command checks**

Run:

```powershell
pnpm rust:test create_batch_rejects_empty_name
pnpm rust:test list_groups_returns_batch_with_provider_and_account_children
pnpm rust:check
```

Expected: PASS and `cargo check` exits with code `0`.

- [ ] **Step 6: Commit services and commands**

```powershell
git add src-tauri/src
git commit -m "feat: add core services and commands"
```

---

### Task 6: Implement Example JSON Import Pipeline

**Files:**
- Create: `fixtures/example-import.json`
- Create: `src-tauri/src/importers/mod.rs`
- Create: `src-tauri/src/importers/example_json.rs`
- Create: `src-tauri/src/services/import_service.rs`
- Create: `src-tauri/src/commands/import_commands.rs`
- Modify: `src-tauri/src/services/mod.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Produces: `ExampleImportPayload { providers, accounts }`.
- Produces: `ImportService::import_example_json(pool, request) -> Result<ImportJob, AppError>`.
- Produces: Tauri command `import_example_json`.

- [ ] **Step 1: Create fixture**

Write `fixtures/example-import.json`:

```json
{
  "providers": [
    {
      "name": "Acme Claude",
      "kind": "openai_compatible",
      "base_url": "https://api.example.com/v1",
      "model_config_json": "{\"default\":\"claude-sonnet\"}",
      "target_options_json": "{}",
      "secret_ref": "secret://provider/acme"
    }
  ],
  "accounts": [
    {
      "platform": "codex",
      "display_name": "Team Account",
      "email": "team@example.com",
      "plan": "team",
      "account_metadata_json": "{\"source\":\"example\"}",
      "secret_ref": "secret://account/team"
    }
  ]
}
```

- [ ] **Step 2: Write importer test**

Add tests in `src-tauri/src/services/import_service.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{create_memory_pool, run_migrations};

    #[tokio::test]
    async fn import_example_json_creates_batch_items_and_job() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let request = ExampleJsonImportRequest {
            batch_name: "Batch 2026-07".to_string(),
            source_label: "inline fixture".to_string(),
            strategy: "skip".to_string(),
            json: r#"{
              "providers": [{"name":"Acme Claude","kind":"openai_compatible","base_url":"https://api.example.com/v1","model_config_json":"{}","target_options_json":"{}","secret_ref":"secret://provider/acme"}],
              "accounts": [{"platform":"codex","display_name":"Team Account","email":"team@example.com","plan":"team","account_metadata_json":"{}","secret_ref":"secret://account/team"}]
            }"#.to_string(),
        };

        let job = ImportService::import_example_json(&pool, request).await.expect("import");

        assert_eq!(job.status, "completed");
        assert_eq!(job.success_count, 2);
        assert_eq!(job.failure_count, 0);
    }
}
```

- [ ] **Step 3: Run importer test to verify failure**

Run:

```powershell
pnpm rust:test import_example_json_creates_batch_items_and_job
```

Expected: FAIL because importer and service are not implemented.

- [ ] **Step 4: Implement example importer**

Write `src-tauri/src/importers/example_json.rs`:

```rust
use crate::models::account::NewOfficialAccount;
use crate::models::provider::NewProvider;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExampleImportPayload {
    #[serde(default)]
    pub providers: Vec<NewProvider>,
    #[serde(default)]
    pub accounts: Vec<NewOfficialAccount>,
}

pub fn parse_example_json(input: &str) -> Result<ExampleImportPayload, serde_json::Error> {
    serde_json::from_str(input)
}
```

Write `src-tauri/src/importers/mod.rs`:

```rust
pub mod example_json;
```

- [ ] **Step 5: Implement import service and command**

Write `src-tauri/src/services/import_service.rs`:

```rust
use crate::database::repositories::batch_repository::BatchRepository;
use crate::database::repositories::import_repository::ImportRepository;
use crate::error::AppError;
use crate::importers::example_json::parse_example_json;
use crate::models::batch::NewBatch;
use crate::models::import_job::ImportJob;
use crate::services::batch_service::BatchService;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExampleJsonImportRequest {
    pub batch_name: String,
    pub source_label: String,
    pub strategy: String,
    pub json: String,
}

pub struct ImportService;

impl ImportService {
    pub async fn import_example_json(pool: &SqlitePool, request: ExampleJsonImportRequest) -> Result<ImportJob, AppError> {
        if request.batch_name.trim().is_empty() {
            return Err(AppError::Validation {
                code: "validation.import_batch_name_required",
                message: "Batch name is required for import".to_string(),
                details: None,
                recoverable: true,
            });
        }

        let payload = parse_example_json(&request.json)?;
        let batch = BatchRepository::create(pool, NewBatch {
            name: request.batch_name.trim().to_string(),
            source: "example_json".to_string(),
            notes: Some(request.source_label.clone()),
        }).await?;

        let job = ImportRepository::create_job(pool, "example_json", &request.source_label, Some(&batch.id), &request.strategy).await?;
        let mut success_count = 0_i64;

        for provider in payload.providers {
            let created = BatchService::create_provider(pool, provider, Some(batch.id.clone())).await?;
            if !created.id.is_empty() {
                success_count += 1;
            }
        }

        for account in payload.accounts {
            let created = BatchService::create_official_account(pool, account, Some(batch.id.clone())).await?;
            if !created.id.is_empty() {
                success_count += 1;
            }
        }

        let summary_json = serde_json::json!({
            "batch_id": batch.id,
            "created": success_count
        }).to_string();

        ImportRepository::complete_job(pool, &job.id, "completed", success_count, 0, 0, &summary_json).await
    }
}
```

Write `src-tauri/src/commands/import_commands.rs`:

```rust
use crate::app_state::AppState;
use crate::error::ApiError;
use crate::models::import_job::ImportJob;
use crate::services::import_service::{ExampleJsonImportRequest, ImportService};
use tauri::State;

#[tauri::command]
pub async fn import_example_json(
    state: State<'_, AppState>,
    request: ExampleJsonImportRequest,
) -> Result<ImportJob, ApiError> {
    ImportService::import_example_json(&state.pool, request).await.map_err(ApiError::from)
}
```

Update `src-tauri/src/services/mod.rs`:

```rust
pub mod batch_service;
pub mod import_service;
pub mod settings_service;
pub mod target_service;
```

Update `src-tauri/src/commands/mod.rs`:

```rust
pub mod batch_commands;
pub mod import_commands;
pub mod settings_commands;
pub mod target_commands;
```

Update `src-tauri/src/lib.rs` with `mod importers;`, import `import_example_json`, and include it in `generate_handler!`.

- [ ] **Step 6: Run importer tests**

Run:

```powershell
pnpm rust:test import_example_json_creates_batch_items_and_job
pnpm rust:check
```

Expected: PASS and `cargo check` exits with code `0`.

- [ ] **Step 7: Commit import pipeline**

```powershell
git add fixtures src-tauri/src
git commit -m "feat: add example json import pipeline"
```

---

### Task 7: Add Atomic Config Writer And Extension Interfaces

**Files:**
- Create: `src-tauri/src/config_writer/mod.rs`
- Create: `src-tauri/src/adapters/mod.rs`
- Create: `src-tauri/src/security/mod.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Produces: `ConfigWriter::write_atomic(path, content) -> Result<WriteOutcome, AppError>`.
- Produces: `TargetAdapter` trait.
- Produces: `QuotaProvider` trait.
- Produces: `SecretStore` trait and `KeyringSecretStore`.

- [ ] **Step 1: Write atomic writer test**

Add tests in `src-tauri/src/config_writer/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn write_atomic_replaces_content_and_reports_hashes() {
        let dir = tempdir().expect("tempdir");
        let target = dir.path().join("config.json");
        tokio::fs::write(&target, "{\"old\":true}").await.expect("seed");

        let outcome = ConfigWriter::write_atomic(&target, "{\"new\":true}").await.expect("write");
        let written = tokio::fs::read_to_string(&target).await.expect("read");

        assert_eq!(written, "{\"new\":true}");
        assert!(outcome.before_hash.is_some());
        assert!(outcome.after_hash.is_some());
        assert_eq!(outcome.status, "written");
    }
}
```

- [ ] **Step 2: Run atomic writer test to verify failure**

Run:

```powershell
pnpm rust:test write_atomic_replaces_content_and_reports_hashes
```

Expected: FAIL because `ConfigWriter` does not exist.

- [ ] **Step 3: Implement config writer**

Write `src-tauri/src/config_writer/mod.rs`:

```rust
use crate::error::AppError;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::Path;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WriteOutcome {
    pub path: String,
    pub before_hash: Option<String>,
    pub after_hash: Option<String>,
    pub status: String,
}

pub struct ConfigWriter;

impl ConfigWriter {
    pub async fn write_atomic(path: &Path, content: &str) -> Result<WriteOutcome, AppError> {
        let parent = path.parent().ok_or_else(|| AppError::Filesystem {
            code: "filesystem.path_parent_missing",
            message: "Target path has no parent directory".to_string(),
            details: Some(path.display().to_string()),
            recoverable: false,
        })?;
        tokio::fs::create_dir_all(parent).await?;

        let before_hash = if path.exists() {
            let before = tokio::fs::read(path).await?;
            Some(hash_bytes(&before))
        } else {
            None
        };

        let temp_path = path.with_extension("tmp.ai-switch");
        let mut file = tokio::fs::File::create(&temp_path).await?;
        file.write_all(content.as_bytes()).await?;
        file.flush().await?;
        drop(file);

        tokio::fs::rename(&temp_path, path).await?;
        let after = tokio::fs::read(path).await?;
        let after_hash = Some(hash_bytes(&after));

        Ok(WriteOutcome {
            path: path.display().to_string(),
            before_hash,
            after_hash,
            status: "written".to_string(),
        })
    }
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}
```

- [ ] **Step 4: Implement extension traits**

Write `src-tauri/src/adapters/mod.rs`:

```rust
use crate::config_writer::WriteOutcome;
use crate::error::AppError;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdapterWriteRequest {
    pub target_key: String,
    pub item_type: String,
    pub item_id: String,
    pub rendered_config: String,
    pub target_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdapterWriteResult {
    pub restart_required: bool,
    pub outcome: WriteOutcome,
}

pub trait TargetAdapter: Send + Sync {
    fn key(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn restart_required(&self) -> bool;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuotaSnapshotDraft {
    pub owner_type: String,
    pub owner_id: String,
    pub status: String,
    pub remaining_label: Option<String>,
    pub reset_at: Option<String>,
    pub summary_json: String,
    pub raw_excerpt_json: String,
}

pub trait QuotaProvider: Send + Sync {
    fn provider_key(&self) -> &'static str;
    fn describe_owner(&self, owner_id: &str) -> Result<String, AppError>;
}

pub struct MockTargetAdapter;

impl TargetAdapter for MockTargetAdapter {
    fn key(&self) -> &'static str {
        "mock"
    }

    fn display_name(&self) -> &'static str {
        "Mock Adapter"
    }

    fn restart_required(&self) -> bool {
        false
    }
}
```

Write `src-tauri/src/security/mod.rs`:

```rust
use crate::error::AppError;

pub trait SecretStore: Send + Sync {
    fn set_secret(&self, key: &str, value: &str) -> Result<(), AppError>;
    fn get_secret(&self, key: &str) -> Result<String, AppError>;
}

pub struct KeyringSecretStore {
    service: String,
}

impl KeyringSecretStore {
    pub fn new(service: impl Into<String>) -> Self {
        Self { service: service.into() }
    }
}

impl SecretStore for KeyringSecretStore {
    fn set_secret(&self, key: &str, value: &str) -> Result<(), AppError> {
        let entry = keyring::Entry::new(&self.service, key).map_err(|err| AppError::Secret {
            code: "secret.entry",
            message: "Could not create keyring entry".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;
        entry.set_password(value).map_err(|err| AppError::Secret {
            code: "secret.set",
            message: "Could not save secret to keyring".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })
    }

    fn get_secret(&self, key: &str) -> Result<String, AppError> {
        let entry = keyring::Entry::new(&self.service, key).map_err(|err| AppError::Secret {
            code: "secret.entry",
            message: "Could not create keyring entry".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })?;
        entry.get_password().map_err(|err| AppError::Secret {
            code: "secret.get",
            message: "Could not read secret from keyring".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })
    }
}
```

Update `src-tauri/src/lib.rs` with:

```rust
mod adapters;
mod config_writer;
mod security;
```

- [ ] **Step 5: Run tests**

Run:

```powershell
pnpm rust:test write_atomic_replaces_content_and_reports_hashes
pnpm rust:check
```

Expected: PASS and `cargo check` exits with code `0`.

- [ ] **Step 6: Commit writer and interfaces**

```powershell
git add src-tauri/src
git commit -m "feat: add config writer and extension interfaces"
```

---

### Task 8: Add Typed Frontend API Client And Shared Types

**Files:**
- Create: `src/lib/api/types.ts`
- Create: `src/lib/api/client.ts`
- Create: `src/lib/query/queryClient.ts`
- Create: `src/test/setup.ts`
- Create: `src/test/fixtures.ts`

**Interfaces:**
- Produces TypeScript types `BatchGroup`, `Provider`, `OfficialAccount`, `ImportJob`, `AppSettings`.
- Produces API functions `listBatchGroups`, `createBatch`, `importExampleJson`, `getSettings`, `saveSettings`, `listTargetApps`.

- [ ] **Step 1: Write API type files**

Write `src/lib/api/types.ts`:

```ts
export type ApiError = {
  code: string;
  message: string;
  details?: string | null;
  recoverable: boolean;
  operation_id?: string | null;
};

export type Batch = {
  id: string;
  name: string;
  source: string;
  notes?: string | null;
  sort_order: number;
  created_at: string;
  updated_at: string;
};

export type BatchChild = {
  item_type: "provider" | "official_account";
  id: string;
  title: string;
  subtitle?: string | null;
  status: "ok" | "warning" | "error";
};

export type BatchGroup = {
  batch: Batch;
  health: "ok" | "warning" | "error";
  children: BatchChild[];
};

export type Provider = {
  id: string;
  name: string;
  kind: string;
  base_url?: string | null;
  model_config_json: string;
  target_options_json: string;
  secret_ref?: string | null;
  status: string;
  sort_order: number;
  created_at: string;
  updated_at: string;
};

export type OfficialAccount = {
  id: string;
  platform: string;
  display_name: string;
  email?: string | null;
  plan?: string | null;
  account_metadata_json: string;
  secret_ref?: string | null;
  quota_snapshot_id?: string | null;
  status: string;
  sort_order: number;
  created_at: string;
  updated_at: string;
};

export type ImportJob = {
  id: string;
  source_type: string;
  source_label: string;
  batch_id?: string | null;
  strategy: string;
  status: string;
  success_count: number;
  failure_count: number;
  conflict_count: number;
  summary_json: string;
  created_at: string;
  completed_at?: string | null;
};

export type TargetApp = {
  id: string;
  key: string;
  display_name: string;
  enabled: number;
  sort_order: number;
  created_at: string;
  updated_at: string;
};

export type AppSettings = {
  language: string;
  theme: string;
  copy_import_sources: boolean;
  logging_enabled: boolean;
  secret_storage: string;
  data_dir: string;
};
```

Write `src/lib/api/client.ts`:

```ts
import { invoke } from "@tauri-apps/api/core";
import type { AppSettings, Batch, BatchGroup, ImportJob, TargetApp } from "./types";

export function listBatchGroups(search?: string): Promise<BatchGroup[]> {
  return invoke("list_batch_groups", { search: search || null });
}

export function createBatch(input: { name: string; source: string; notes?: string | null }): Promise<Batch> {
  return invoke("create_batch", { input });
}

export function importExampleJson(request: {
  batch_name: string;
  source_label: string;
  strategy: string;
  json: string;
}): Promise<ImportJob> {
  return invoke("import_example_json", { request });
}

export function listTargetApps(): Promise<TargetApp[]> {
  return invoke("list_target_apps");
}

export function getSettings(): Promise<AppSettings> {
  return invoke("get_settings");
}

export function saveSettings(settings: AppSettings): Promise<AppSettings> {
  return invoke("save_settings", { settings });
}
```

Write `src/lib/query/queryClient.ts`:

```ts
import { QueryClient } from "@tanstack/react-query";

export function createQueryClient() {
  return new QueryClient({
    defaultOptions: {
      queries: {
        retry: 1,
        staleTime: 10_000,
      },
    },
  });
}
```

- [ ] **Step 2: Add frontend test setup and fixtures**

Write `src/test/setup.ts`:

```ts
import "@testing-library/jest-dom/vitest";
```

Write `src/test/fixtures.ts`:

```ts
import type { AppSettings, BatchGroup } from "../lib/api/types";

export const batchGroupsFixture: BatchGroup[] = [
  {
    batch: {
      id: "batch-1",
      name: "July imports",
      source: "example_json",
      notes: null,
      sort_order: 0,
      created_at: "2026-07-13T00:00:00Z",
      updated_at: "2026-07-13T00:00:00Z",
    },
    health: "ok",
    children: [
      {
        item_type: "provider",
        id: "provider-1",
        title: "Acme Claude",
        subtitle: "openai_compatible",
        status: "ok",
      },
      {
        item_type: "official_account",
        id: "account-1",
        title: "Team Account",
        subtitle: "team@example.com",
        status: "ok",
      },
    ],
  },
];

export const settingsFixture: AppSettings = {
  language: "zh-CN",
  theme: "system",
  copy_import_sources: false,
  logging_enabled: true,
  secret_storage: "keyring",
  data_dir: "C:/Users/example/.ai-switch",
};
```

- [ ] **Step 3: Run typecheck**

Run:

```powershell
pnpm typecheck
pnpm test:run -- --passWithNoTests
```

Expected: PASS.

- [ ] **Step 4: Commit frontend API foundation**

```powershell
git add src/lib src/test
git commit -m "feat: add typed frontend api client"
```

---

### Task 9: Build Batch-First UI And Core Screens

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/components/layout/AppLayout.tsx`
- Create: `src/components/batches/BatchList.tsx`
- Create: `src/screens/DashboardScreen.tsx`
- Create: `src/screens/BatchesScreen.tsx`
- Create: `src/screens/ProvidersScreen.tsx`
- Create: `src/screens/AccountsScreen.tsx`
- Create: `tests/BatchList.test.tsx`

**Interfaces:**
- Produces: `BatchList({ groups, search })`.
- Produces: screen navigation in `App`.

- [ ] **Step 1: Write BatchList test**

Write `tests/BatchList.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { BatchList } from "../src/components/batches/BatchList";
import { batchGroupsFixture } from "../src/test/fixtures";

describe("BatchList", () => {
  it("renders batches collapsed and expands child items", async () => {
    render(<BatchList groups={batchGroupsFixture} search="" />);

    expect(screen.getByText("July imports")).toBeInTheDocument();
    expect(screen.queryByText("Acme Claude")).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: /expand July imports/i }));

    expect(screen.getByText("Acme Claude")).toBeInTheDocument();
    expect(screen.getByText("Team Account")).toBeInTheDocument();
  });

  it("auto expands when search matches a child", () => {
    render(<BatchList groups={batchGroupsFixture} search="team@example.com" />);

    expect(screen.getByText("Team Account")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run UI test to verify failure**

Run:

```powershell
pnpm test:run tests/BatchList.test.tsx
```

Expected: FAIL because `BatchList` does not exist.

- [ ] **Step 3: Implement BatchList**

Write `src/components/batches/BatchList.tsx`:

```tsx
import { ChevronDown, ChevronRight } from "lucide-react";
import { useState } from "react";
import type { BatchGroup } from "../../lib/api/types";

type BatchListProps = {
  groups: BatchGroup[];
  search: string;
};

export function BatchList({ groups, search }: BatchListProps) {
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const normalizedSearch = search.trim().toLowerCase();

  if (groups.length === 0) {
    return (
      <div className="rounded-3xl border border-dashed border-ink/20 bg-white/50 p-8 text-center text-steel">
        No batches or records yet.
      </div>
    );
  }

  return (
    <div className="space-y-3">
      {groups.map((group) => {
        const searchMatchesChild =
          normalizedSearch.length > 0 &&
          group.children.some((child) =>
            `${child.title} ${child.subtitle ?? ""}`.toLowerCase().includes(normalizedSearch),
          );
        const isExpanded = expanded[group.batch.id] || searchMatchesChild;

        return (
          <section key={group.batch.id} className="rounded-3xl border border-ink/10 bg-white/75 p-4 shadow-sm">
            <button
              type="button"
              aria-label={`expand ${group.batch.name}`}
              className="flex w-full items-center justify-between text-left"
              onClick={() =>
                setExpanded((current) => ({
                  ...current,
                  [group.batch.id]: !isExpanded,
                }))
              }
            >
              <span className="flex items-center gap-3">
                {isExpanded ? <ChevronDown size={18} /> : <ChevronRight size={18} />}
                <span>
                  <span className="block font-display text-lg font-semibold text-ink">{group.batch.name}</span>
                  <span className="text-sm text-steel">
                    {group.batch.source} · {group.children.length} item{group.children.length === 1 ? "" : "s"}
                  </span>
                </span>
              </span>
              <span className="rounded-full bg-moss/10 px-3 py-1 text-xs font-semibold uppercase tracking-wide text-moss">
                {group.health}
              </span>
            </button>

            {isExpanded && (
              <div className="mt-4 divide-y divide-ink/10 overflow-hidden rounded-2xl border border-ink/10">
                {group.children.map((child) => (
                  <div key={`${child.item_type}:${child.id}`} className="flex items-center justify-between bg-paper/50 px-4 py-3">
                    <div>
                      <p className="font-medium text-ink">{child.title}</p>
                      <p className="text-sm text-steel">{child.subtitle ?? child.item_type}</p>
                    </div>
                    <span className="rounded-full bg-white px-3 py-1 text-xs text-steel">{child.item_type}</span>
                  </div>
                ))}
              </div>
            )}
          </section>
        );
      })}
    </div>
  );
}
```

- [ ] **Step 4: Implement screens and navigation**

Write `src/screens/DashboardScreen.tsx`:

```tsx
export function DashboardScreen() {
  return (
    <section className="rounded-3xl border border-ink/10 bg-white/70 p-8 shadow-xl shadow-ink/5">
      <p className="text-sm uppercase tracking-[0.3em] text-moss">Foundation</p>
      <h1 className="mt-3 font-display text-4xl font-semibold text-ink">AI Switch</h1>
      <p className="mt-4 max-w-2xl text-base leading-7 text-steel">
        Batch-first provider and official account switching foundation.
      </p>
    </section>
  );
}
```

Write `src/screens/BatchesScreen.tsx`:

```tsx
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { BatchList } from "../components/batches/BatchList";
import { listBatchGroups } from "../lib/api/client";

export function BatchesScreen() {
  const [search, setSearch] = useState("");
  const groupsQuery = useQuery({
    queryKey: ["batch-groups", search],
    queryFn: () => listBatchGroups(search),
  });

  return (
    <section className="space-y-4">
      <div>
        <h1 className="font-display text-3xl font-semibold text-ink">Batches</h1>
        <p className="text-steel">Imported providers and official accounts are grouped by batch.</p>
      </div>
      <input
        value={search}
        onChange={(event) => setSearch(event.target.value)}
        aria-label="Search batches, accounts, providers"
        className="w-full rounded-2xl border border-ink/10 bg-white/80 px-4 py-3 outline-none focus:border-moss"
      />
      {groupsQuery.isLoading && <p className="text-steel">Loading batches...</p>}
      {groupsQuery.error && <p className="text-ember">Could not load batches.</p>}
      {groupsQuery.data && <BatchList groups={groupsQuery.data} search={search} />}
    </section>
  );
}
```

Write `src/screens/ProvidersScreen.tsx`:

```tsx
export function ProvidersScreen() {
  return <div className="rounded-3xl bg-white/70 p-6">Provider-focused management will use the batch foundation.</div>;
}
```

Write `src/screens/AccountsScreen.tsx`:

```tsx
export function AccountsScreen() {
  return <div className="rounded-3xl bg-white/70 p-6">Official account management will use metadata-only account records in Phase A.</div>;
}
```

Update `src/components/layout/AppLayout.tsx`:

```tsx
type AppLayoutProps = {
  children: React.ReactNode;
  activeScreen: string;
  onNavigate: (screen: string) => void;
};

const screens = ["Dashboard", "Batches", "Providers", "Accounts", "Imports", "Targets", "Settings", "Log"];

export function AppLayout({ children, activeScreen, onNavigate }: AppLayoutProps) {
  return (
    <main className="min-h-screen bg-[radial-gradient(circle_at_top_left,#f8ddc8,transparent_32%),linear-gradient(135deg,#f4efe5,#dfe7df)] px-6 py-8 font-body text-ink">
      <div className="mx-auto grid max-w-7xl gap-6 lg:grid-cols-[240px_1fr]">
        <aside className="rounded-3xl border border-ink/10 bg-white/60 p-4 shadow-sm">
          <p className="px-3 pb-4 font-display text-2xl font-semibold">AI Switch</p>
          <nav className="space-y-1">
            {screens.map((screen) => (
              <button
                key={screen}
                type="button"
                onClick={() => onNavigate(screen)}
                className={`w-full rounded-2xl px-3 py-2 text-left text-sm font-medium ${
                  activeScreen === screen ? "bg-ink text-paper" : "text-steel hover:bg-white"
                }`}
              >
                {screen}
              </button>
            ))}
          </nav>
        </aside>
        <div>{children}</div>
      </div>
    </main>
  );
}
```

Update `src/App.tsx`:

```tsx
import { QueryClientProvider } from "@tanstack/react-query";
import { useState } from "react";
import { AppLayout } from "./components/layout/AppLayout";
import { createQueryClient } from "./lib/query/queryClient";
import { AccountsScreen } from "./screens/AccountsScreen";
import { BatchesScreen } from "./screens/BatchesScreen";
import { DashboardScreen } from "./screens/DashboardScreen";
import { ProvidersScreen } from "./screens/ProvidersScreen";

const queryClient = createQueryClient();

export function App() {
  const [screen, setScreen] = useState("Dashboard");

  return (
    <QueryClientProvider client={queryClient}>
      <AppLayout activeScreen={screen} onNavigate={setScreen}>
        {screen === "Dashboard" && <DashboardScreen />}
        {screen === "Batches" && <BatchesScreen />}
        {screen === "Providers" && <ProvidersScreen />}
        {screen === "Accounts" && <AccountsScreen />}
        {!["Dashboard", "Batches", "Providers", "Accounts"].includes(screen) && (
          <div className="rounded-3xl bg-white/70 p-6">{screen} foundation screen.</div>
        )}
      </AppLayout>
    </QueryClientProvider>
  );
}
```

- [ ] **Step 5: Run UI tests**

Run:

```powershell
pnpm test:run tests/BatchList.test.tsx
pnpm typecheck
```

Expected: PASS.

- [ ] **Step 6: Commit batch UI**

```powershell
git add src tests
git commit -m "feat: add batch-first ui"
```

---

### Task 10: Add Import, Settings, Targets, And Log Screens

**Files:**
- Create: `src/components/imports/ImportPanel.tsx`
- Create: `src/screens/ImportsScreen.tsx`
- Create: `src/screens/TargetsScreen.tsx`
- Create: `src/screens/SettingsScreen.tsx`
- Create: `src/screens/OperationLogScreen.tsx`
- Modify: `src/App.tsx`
- Create: `tests/ImportPanel.test.tsx`
- Create: `tests/SettingsScreen.test.tsx`

**Interfaces:**
- Produces: `ImportPanel({ onImport })`.
- Produces screens that call existing API functions.

- [ ] **Step 1: Write ImportPanel test**

Write `tests/ImportPanel.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ImportPanel } from "../src/components/imports/ImportPanel";

describe("ImportPanel", () => {
  it("requires a batch name before import", async () => {
    const onImport = vi.fn();
    render(<ImportPanel onImport={onImport} />);

    await userEvent.type(screen.getByLabelText(/json/i), "{\"providers\":[],\"accounts\":[]}");
    await userEvent.click(screen.getByRole("button", { name: /import/i }));

    expect(screen.getByText("Batch name is required.")).toBeInTheDocument();
    expect(onImport).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: Implement ImportPanel**

Write `src/components/imports/ImportPanel.tsx`:

```tsx
import { useState } from "react";
import { Button } from "../ui/Button";

type ImportPanelProps = {
  onImport: (request: { batch_name: string; source_label: string; strategy: string; json: string }) => Promise<void>;
};

export function ImportPanel({ onImport }: ImportPanelProps) {
  const [batchName, setBatchName] = useState("");
  const [sourceLabel, setSourceLabel] = useState("manual paste");
  const [json, setJson] = useState("{\"providers\":[],\"accounts\":[]}");
  const [error, setError] = useState<string | null>(null);

  async function submit() {
    if (batchName.trim().length === 0) {
      setError("Batch name is required.");
      return;
    }
    setError(null);
    await onImport({
      batch_name: batchName.trim(),
      source_label: sourceLabel.trim() || "manual paste",
      strategy: "skip",
      json,
    });
  }

  return (
    <div className="space-y-4 rounded-3xl border border-ink/10 bg-white/75 p-5">
      <label className="block text-sm font-semibold text-ink">
        Batch name
        <input
          value={batchName}
          onChange={(event) => setBatchName(event.target.value)}
          className="mt-2 w-full rounded-2xl border border-ink/10 px-4 py-3"
        />
      </label>
      <label className="block text-sm font-semibold text-ink">
        Source label
        <input
          value={sourceLabel}
          onChange={(event) => setSourceLabel(event.target.value)}
          className="mt-2 w-full rounded-2xl border border-ink/10 px-4 py-3"
        />
      </label>
      <label className="block text-sm font-semibold text-ink">
        JSON
        <textarea
          value={json}
          onChange={(event) => setJson(event.target.value)}
          rows={8}
          className="mt-2 w-full rounded-2xl border border-ink/10 px-4 py-3 font-mono text-sm"
        />
      </label>
      {error && <p className="text-sm font-medium text-ember">{error}</p>}
      <Button type="button" onClick={submit}>Import</Button>
    </div>
  );
}
```

- [ ] **Step 3: Implement remaining screens**

Write `src/screens/ImportsScreen.tsx`:

```tsx
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { ImportPanel } from "../components/imports/ImportPanel";
import { importExampleJson } from "../lib/api/client";

export function ImportsScreen() {
  const queryClient = useQueryClient();
  const importMutation = useMutation({
    mutationFn: importExampleJson,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["batch-groups"] }),
  });

  return (
    <section className="space-y-4">
      <div>
        <h1 className="font-display text-3xl font-semibold text-ink">Imports</h1>
        <p className="text-steel">Paste example JSON and assign it to a named batch.</p>
      </div>
      <ImportPanel onImport={(request) => importMutation.mutateAsync(request)} />
      {importMutation.data && (
        <p className="rounded-2xl bg-moss/10 p-4 text-moss">
          Imported {importMutation.data.success_count} records into batch {importMutation.data.batch_id}.
        </p>
      )}
      {importMutation.error && <p className="text-ember">Import failed.</p>}
    </section>
  );
}
```

Write `src/screens/TargetsScreen.tsx`:

```tsx
import { useQuery } from "@tanstack/react-query";
import { listTargetApps } from "../lib/api/client";

export function TargetsScreen() {
  const targetsQuery = useQuery({ queryKey: ["targets"], queryFn: listTargetApps });

  return (
    <section className="space-y-4">
      <h1 className="font-display text-3xl font-semibold text-ink">Targets</h1>
      {targetsQuery.isLoading && <p className="text-steel">Loading targets...</p>}
      <div className="grid gap-3 sm:grid-cols-2">
        {targetsQuery.data?.map((target) => (
          <div key={target.id} className="rounded-3xl border border-ink/10 bg-white/70 p-4">
            <p className="font-semibold">{target.display_name}</p>
            <p className="text-sm text-steel">{target.key}</p>
          </div>
        ))}
      </div>
    </section>
  );
}
```

Write `src/screens/SettingsScreen.tsx`:

```tsx
import { useMutation, useQuery } from "@tanstack/react-query";
import { getSettings, saveSettings } from "../lib/api/client";

export function SettingsScreen() {
  const settingsQuery = useQuery({ queryKey: ["settings"], queryFn: getSettings });
  const saveMutation = useMutation({ mutationFn: saveSettings });

  if (settingsQuery.isLoading) {
    return <p className="text-steel">Loading settings...</p>;
  }

  if (!settingsQuery.data) {
    return <p className="text-ember">Could not load settings.</p>;
  }

  const settings = settingsQuery.data;

  return (
    <section className="space-y-4 rounded-3xl border border-ink/10 bg-white/70 p-6">
      <h1 className="font-display text-3xl font-semibold text-ink">Settings</h1>
      <p className="text-sm text-steel">Data directory: {settings.data_dir}</p>
      <button
        type="button"
        className="rounded-full bg-ink px-4 py-2 text-sm font-semibold text-paper"
        onClick={() => saveMutation.mutate({ ...settings, theme: settings.theme === "dark" ? "system" : "dark" })}
      >
        Toggle theme value
      </button>
      {saveMutation.data && <p className="text-moss">Settings saved.</p>}
    </section>
  );
}
```

Write `src/screens/OperationLogScreen.tsx`:

```tsx
export function OperationLogScreen() {
  return (
    <section className="rounded-3xl border border-ink/10 bg-white/70 p-6">
      <h1 className="font-display text-3xl font-semibold text-ink">Operation Log</h1>
      <p className="mt-2 text-steel">Import and config write events appear here when services emit them.</p>
    </section>
  );
}
```

Update `src/App.tsx` imports and screen rendering:

```tsx
import { ImportsScreen } from "./screens/ImportsScreen";
import { OperationLogScreen } from "./screens/OperationLogScreen";
import { SettingsScreen } from "./screens/SettingsScreen";
import { TargetsScreen } from "./screens/TargetsScreen";
```

Add these render branches:

```tsx
{screen === "Imports" && <ImportsScreen />}
{screen === "Targets" && <TargetsScreen />}
{screen === "Settings" && <SettingsScreen />}
{screen === "Log" && <OperationLogScreen />}
```

Change the fallback condition to:

```tsx
{!["Dashboard", "Batches", "Providers", "Accounts", "Imports", "Targets", "Settings", "Log"].includes(screen) && (
  <div className="rounded-3xl bg-white/70 p-6">{screen} foundation screen.</div>
)}
```

- [ ] **Step 4: Run frontend tests**

Run:

```powershell
pnpm test:run tests/ImportPanel.test.tsx tests/BatchList.test.tsx
pnpm typecheck
```

Expected: PASS.

- [ ] **Step 5: Commit import and settings UI**

```powershell
git add src tests
git commit -m "feat: add import and settings screens"
```

---

### Task 11: Add Final Verification, README Updates, And Smoke Checklist

**Files:**
- Modify: `README.md`
- Create: `docs/superpowers/plans/2026-07-13-ai-switch-foundation-smoke.md`

**Interfaces:**
- Produces documented commands for local development and Phase A verification.

- [ ] **Step 1: Update README**

Replace `README.md` with:

```markdown
# ai-switch

AI Switch is a Tauri-based desktop foundation for AI provider and official account switching.

Phase A includes:

- Tauri 2 + React + TypeScript app shell
- Rust backend with typed Tauri commands
- SQLite foundation schema
- Batch-first provider and account grouping
- Example JSON import into a named batch
- Settings stored in `~/.ai-switch/settings.json`
- Atomic config writer primitives
- Extension interfaces for target adapters, importers, quota providers, and secret storage

## Development

Install dependencies:

```powershell
corepack enable
pnpm install
```

Run frontend checks:

```powershell
pnpm typecheck
pnpm test:run
```

Run Rust checks:

```powershell
pnpm rust:check
pnpm rust:test
```

Run the desktop app in development mode:

```powershell
pnpm tauri:dev
```

## Clean-Room Boundary

This project may study public behavior, public documentation, and public file formats from related tools. It must not copy or translate non-commercial source code from `cockpit-tools`.
```

- [ ] **Step 2: Add smoke checklist**

Write `docs/superpowers/plans/2026-07-13-ai-switch-foundation-smoke.md`:

```markdown
# AI Switch Foundation Smoke Checklist

Run these checks after completing Phase A implementation.

- `pnpm typecheck` exits with code `0`.
- `pnpm test:run` exits with code `0`.
- `pnpm rust:check` exits with code `0`.
- `pnpm rust:test` exits with code `0`.
- `pnpm tauri:dev` opens the app window.
- Navigate to `Batches`; the empty state renders.
- Navigate to `Imports`; paste `fixtures/example-import.json`, enter batch name `Manual July Batch`, and import.
- Navigate to `Batches`; the new batch appears.
- Expand the batch; `Acme Claude` and `Team Account` appear.
- Navigate to `Targets`; the seven default target apps appear.
- Navigate to `Settings`; the data directory path is visible.
```

- [ ] **Step 3: Run full verification**

Run:

```powershell
pnpm typecheck
pnpm test:run
pnpm rust:check
pnpm rust:test
```

Expected: all commands exit with code `0`.

- [ ] **Step 4: Commit documentation**

```powershell
git add README.md docs/superpowers/plans/2026-07-13-ai-switch-foundation-smoke.md
git commit -m "docs: add foundation verification guide"
```

---

## Final Implementation Verification

After all tasks are complete, run:

```powershell
git status --short
pnpm typecheck
pnpm test:run
pnpm rust:check
pnpm rust:test
```

Expected:

- `git status --short` is clean after the final commit.
- TypeScript typecheck passes.
- Frontend tests pass.
- Rust check passes.
- Rust tests pass.

If `pnpm tauri:dev` is run manually, verify the smoke checklist in `docs/superpowers/plans/2026-07-13-ai-switch-foundation-smoke.md`.

## Spec Coverage Map

- Tauri 2 app shell: Task 1.
- React and TypeScript frontend: Tasks 1, 8, 9, 10.
- SQLite business data source of truth: Tasks 3 and 4.
- JSON device-level settings: Task 2 and Task 10.
- Unified models: Task 3.
- Batch-first listing: Task 4 and Task 9.
- Example JSON import with batch name: Task 6 and Task 10.
- Import jobs: Task 4 and Task 6.
- Atomic config writer: Task 7.
- Adapter, importer, quota, and secret extension interfaces: Task 7.
- Rust and frontend tests: Tasks 2 through 11.
- Clean-room licensing boundary: Global constraints and README update in Task 11.
