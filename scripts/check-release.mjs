import { existsSync, readFileSync, statSync } from "node:fs";
import path from "node:path";

const root = process.cwd();
const errors = [];
const warnings = [];

function readJson(relativePath) {
  const fullPath = path.join(root, relativePath);
  try {
    return JSON.parse(readFileSync(fullPath, "utf8"));
  } catch (error) {
    errors.push(`${relativePath}: ${error.message}`);
    return {};
  }
}

function readText(relativePath) {
  const fullPath = path.join(root, relativePath);
  try {
    return readFileSync(fullPath, "utf8");
  } catch (error) {
    errors.push(`${relativePath}: ${error.message}`);
    return "";
  }
}

function getTomlSection(text, sectionName) {
  const lines = text.split(/\r?\n/);
  const sectionHeader = `[${sectionName}]`;
  const sectionLines = [];
  let inSection = false;

  for (const line of lines) {
    const trimmed = line.trim();
    if (trimmed.startsWith("[") && trimmed.endsWith("]")) {
      if (inSection) {
        break;
      }
      inSection = trimmed === sectionHeader;
      continue;
    }
    if (inSection) {
      sectionLines.push(line);
    }
  }

  return sectionLines.join("\n");
}

function getTomlString(sectionText, key) {
  const match = sectionText.match(new RegExp(`^\\s*${key}\\s*=\\s*"([^"]+)"`, "m"));
  return match?.[1];
}

function expect(condition, message) {
  if (!condition) {
    errors.push(message);
  }
}

function warn(condition, message) {
  if (!condition) {
    warnings.push(message);
  }
}

const packageJson = readJson("package.json");
const tauriConf = readJson("src-tauri/tauri.conf.json");
const cargoToml = readText("src-tauri/Cargo.toml");
const cargoPackage = getTomlSection(cargoToml, "package");
const cargoVersion = getTomlString(cargoPackage, "version");
const cargoName = getTomlString(cargoPackage, "name");
const scripts = packageJson.scripts ?? {};

expect(packageJson.private === true, "package.json should remain private for desktop release packaging.");
expect(Boolean(packageJson.packageManager), "package.json should pin packageManager for reproducible installs.");
expect(Boolean(tauriConf.productName), "src-tauri/tauri.conf.json must define productName.");
expect(Boolean(tauriConf.identifier), "src-tauri/tauri.conf.json must define identifier.");
expect(Boolean(tauriConf.version), "src-tauri/tauri.conf.json must define version.");
expect(tauriConf.version === packageJson.version, "package.json version must match tauri.conf.json version.");
expect(cargoVersion === packageJson.version, "package.json version must match src-tauri/Cargo.toml package.version.");
expect(cargoName === packageJson.name, "package.json name must match src-tauri/Cargo.toml package.name.");
expect(tauriConf.build?.frontendDist === "../dist", "tauri build.frontendDist should point at ../dist.");
expect(tauriConf.build?.beforeBuildCommand === "pnpm build", "tauri beforeBuildCommand should run pnpm build.");
expect(typeof tauriConf.identifier === "string" && tauriConf.identifier.split(".").length >= 3, "tauri identifier should use reverse-DNS format.");
expect(Array.isArray(tauriConf.bundle?.icon), "tauri bundle.icon should list generated application icons.");
expect(tauriConf.bundle?.icon?.includes("icons/icon.ico"), "tauri bundle.icon must include icons/icon.ico for Windows MSI bundling.");

for (const scriptName of [
  "typecheck",
  "test:run",
  "rust:check",
  "rust:test",
  "tauri:build",
  "tauri:bundle:windows",
  "release:verify",
  "release:build",
  "release:bundle:windows",
]) {
  expect(Boolean(scripts[scriptName]), `package.json scripts.${scriptName} is required.`);
}

const iconPath = path.join(root, "src-tauri", "icons", "icon.ico");
expect(existsSync(iconPath), "src-tauri/icons/icon.ico is required for Windows bundling.");

if (existsSync(iconPath)) {
  const iconSize = statSync(iconPath).size;
  warn(iconSize >= 1024, `src-tauri/icons/icon.ico is only ${iconSize} bytes; replace the placeholder before public distribution.`);
}

warn(tauriConf.bundle?.active === false, "Default tauri bundle.active should stay false; use tauri:bundle:windows for installer output.");
warn(Boolean(scripts["tauri:bundle:windows"]?.includes("--no-sign")), "Windows bundle script should stay unsigned until signing credentials are configured.");
expect(existsSync(path.join(root, ".github", "workflows", "release.yml")), ".github/workflows/release.yml is required for CI release verification.");

if (warnings.length > 0) {
  console.log("Release readiness warnings:");
  for (const warning of warnings) {
    console.log(`- ${warning}`);
  }
}

if (errors.length > 0) {
  console.error("Release readiness failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log("Release readiness checks passed.");
