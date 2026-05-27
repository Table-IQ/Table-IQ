import { useCallback, useEffect, useState } from "react";
import {
  downloadText,
  installSamplePlugin,
  listPlugins,
  runPluginOnResult,
  type PluginInfo,
} from "../lib/commands";
import { Panel } from "./ui/Panel";
import { IconExport, IconPlugins } from "./ui/Icon";

export function PluginsPanel({
  resultId,
  onClose,
}: {
  resultId: string;
  onClose: () => void;
}) {
  const [plugins, setPlugins] = useState<PluginInfo[]>([]);
  const [output, setOutput] = useState<string | null>(null);
  const [status, setStatus] = useState<string>("");

  const refresh = useCallback(() => listPlugins().then(setPlugins).catch(() => {}), []);
  useEffect(() => {
    refresh();
  }, [refresh]);

  const run = async (p: PluginInfo) => {
    setStatus(`Running ${p.name}…`);
    setOutput(null);
    try {
      setOutput(await runPluginOnResult(p.id, resultId));
      setStatus("");
    } catch (e) {
      setStatus(String(e));
    }
  };

  return (
    <Panel
      title="Plugins"
      onClose={onClose}
      width="md"
      toolbar={
        <div className="flex items-center gap-2 border-b border-border bg-surface px-4 py-2 text-xs">
          <IconPlugins size={12} className="text-muted" />
          <span className="text-muted">Run a WASM plugin over this result.</span>
          <button
            onClick={async () => {
              setStatus(
                await installSamplePlugin()
                  .then(() => "Sample installed")
                  .catch((e) => String(e)),
              );
              refresh();
            }}
            className="ml-auto rounded-md border border-border px-2 py-1 text-xs text-muted hover:text-text"
            title="Install the bundled markdown-export sample plugin"
          >
            Install sample
          </button>
        </div>
      }
    >
      <div className="p-4">
        {plugins.length === 0 ? (
          <div className="rounded-md border border-dashed border-border p-4 text-center text-xs text-muted">
            No plugins. Drop a <code>.wasm</code> into the plugins dir, or install the sample above.
          </div>
        ) : (
          <div className="mb-3 space-y-1">
            {plugins.map((p) => (
              <button
                key={p.id}
                onClick={() => run(p)}
                className="flex w-full items-center gap-2 rounded-md border border-border bg-bg px-2.5 py-2 text-left hover:border-primary hover:bg-surface"
              >
                <span className="min-w-0 flex-1">
                  <span className="block text-sm text-text">{p.name}</span>
                  <span className="block truncate text-[11px] text-muted">{p.description}</span>
                </span>
                <span className="rounded bg-surface-2 px-1.5 py-0.5 text-[10px] uppercase text-muted">
                  {p.kind}
                </span>
              </button>
            ))}
          </div>
        )}
        {status && <div className="mb-2 text-xs text-muted">{status}</div>}
        {output !== null && (
          <div>
            <div className="mb-1 flex items-center justify-between">
              <span className="text-[10px] font-semibold uppercase tracking-wider text-muted">
                Output
              </span>
              <button
                onClick={() => downloadText("plugin-output.txt", output, "text/plain")}
                className="inline-flex items-center gap-1 text-xs text-primary hover:underline"
              >
                <IconExport size={11} /> Download
              </button>
            </div>
            <pre className="max-h-60 overflow-auto rounded-md border border-border bg-surface p-3 font-mono text-[11px] text-text">
              {output}
            </pre>
          </div>
        )}
      </div>
    </Panel>
  );
}
