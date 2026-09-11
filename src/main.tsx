import React from "react";
import ReactDOM from "react-dom/client";
import { ApplicationEntry } from "./ApplicationEntry";
import { applyThemePreference, readStoredThemePreference } from "./lib/theme";
import "virtual:uno.css";
import "./styles.css";

// Apply the last known theme before the first paint so a dark preference never
// flashes the light background on startup.
applyThemePreference(readStoredThemePreference() ?? "system");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ApplicationEntry />
  </React.StrictMode>,
);
