import type { McpAppType } from "../../lib/api/types";
import { MCP_APPS } from "./catalog";

type McpAppSelectorProps = {
  selectedApps: McpAppType[];
  onChange: (apps: McpAppType[]) => void;
  legend: string;
};

export function McpAppSelector({ selectedApps, onChange, legend }: McpAppSelectorProps) {
  return (
    <fieldset>
      <legend className="text-[12px] font-semibold text-stone-800">{legend}</legend>
      <div className="mt-2 grid grid-cols-2 gap-2 sm:grid-cols-3">
        {MCP_APPS.map((app) => (
          <label className="flex items-center gap-2 border border-stone-200 px-2 py-1.5 text-[11px]" key={app.id}>
            <input
              checked={selectedApps.includes(app.id)}
              onChange={(event) =>
                onChange(
                  event.target.checked
                    ? [...selectedApps, app.id]
                    : selectedApps.filter((value) => value !== app.id),
                )
              }
              type="checkbox"
            />
            {app.label}
          </label>
        ))}
      </div>
    </fieldset>
  );
}
