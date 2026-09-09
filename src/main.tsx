import React from "react";
import ReactDOM from "react-dom/client";
import { ApplicationEntry } from "./ApplicationEntry";
import "virtual:uno.css";
import "./styles.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ApplicationEntry />
  </React.StrictMode>,
);
