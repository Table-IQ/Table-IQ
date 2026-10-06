import { useCallback, useEffect, useState } from "react";
import { docCreateIndex, docDropIndex, docIndexes, type DocIndex } from "../lib/commands";
import { confirmDialog } from "../lib/confirmDialog";
import { keysText, parseObject, readableError } from "../lib/docJson";
import { Chip } from "./ui/Chip";
import { IconRefresh, IconTrash } from "./ui/Icon";

const input =
  "h-6 rounded-md border border-border bg-bg px-1.5 font-mono text-[11.5px] text-text placeholder:text-muted-2 focus:border-primary focus:outline-none";

/** A collection's indexes: list, create (keys JSON · unique · name), drop (confirmed). */
export function DocIndexes({ connId, collection, active }: { connId: string; collection: string; active: boolean }) {
  const [rows, setRows] = useState<DocIndex[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [keys, setKeys] = useState("");
  const [unique, setUnique] = useState(false);
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);

  const load = useCallback(() => {
    docIndexes(connId, collection)
      .then((r) => {
        setRows(r);
        setError(null);
      })
      .catch((e) => setError(readableError(e)));
  }, [connId, collection]);
  useEffect(() => {
    if (active) load();
  }, [active, load]);

  const create = async (e: React.FormEvent) => {
    e.preventDefault();
    const p = parseObject(keys);
    if (!p.ok) return setError(`Keys: ${p.error}`);
    if (Object.keys(p.value).length === 0) return setError("Keys: add at least one field, like {\"email\": 1}");
    setBusy(true);
    try {
      await docCreateIndex(connId, collection, keys, unique, name.trim() || undefined);
      setKeys("");
      setName("");
      setUnique(false);
      load();
    } catch (err) {
      setError(readableError(err));
    } finally {
      setBusy(false);
    }
  };

  const drop = async (ix: DocIndex) => {
    if (!(await confirmDialog(`Drop index "${ix.name}" on ${collection}? Queries that rely on it will get slower.`, "Drop index", { ok: "Drop index", cancel: "Cancel" }))) return;
    try {
      await docDropIndex(connId, collection, ix.name);
      load();
    } catch (err) {
      setError(readableError(err));
    }
  };

  return (
    <div className={`${active ? "flex" : "hidden"} h-full min-h-0 flex-col`} data-testid="doc-indexes">
      <form onSubmit={create} className="tiq-rail flex flex-wrap items-center gap-1.5 border-b border-border bg-bg px-3 py-1.5 text-[11px] text-muted">
        <label className="flex min-w-[220px] flex-1 items-center gap-1">
          <span>Keys</span>
          <input aria-label="Index keys" value={keys} onChange={(e) => { setKeys(e.target.value); setError(null); }} placeholder={`{"email": 1}`} spellCheck={false} className={`${input} min-w-0 flex-1`} />
        </label>
        <label className="flex items-center gap-1">
          <span>Name</span>
          <input aria-label="Index name" value={name} onChange={(e) => setName(e.target.value)} placeholder="optional" spellCheck={false} className={`${input} w-36`} />
        </label>
        <label className="flex items-center gap-1">
          <input type="checkbox" checked={unique} onChange={(e) => setUnique(e.target.checked)} className="accent-[var(--primary)]" />
          <span>Unique</span>
        </label>
        <button type="submit" disabled={busy || keys.trim() === ""} className="h-6 rounded-md bg-primary px-2.5 text-[11px] font-medium text-white hover:opacity-90 disabled:opacity-40">
          Create index
        </button>
        <button type="button" title="Refresh" aria-label="Refresh indexes" onClick={load} className="inline-flex h-6 w-6 items-center justify-center rounded-md text-muted hover:bg-surface-2 hover:text-text">
          <IconRefresh size={12} />
        </button>
      </form>
      {error && (
        <div role="alert" className="border-b border-border bg-danger/10 px-3 py-1.5 text-[11px] text-danger">
          {error}
        </div>
      )}
      <div className="min-h-0 flex-1 overflow-auto">
        <table className="w-full text-xs">
          <thead className="border-b border-border bg-surface text-left text-muted">
            <tr>
              <th className="px-3 py-1.5 font-medium">Name</th>
              <th className="px-3 py-1.5 font-medium">Keys</th>
              <th className="px-3 py-1.5 font-medium">Options</th>
              <th className="w-10" />
            </tr>
          </thead>
          <tbody className="text-text">
            {(rows ?? []).map((ix) => (
              <tr key={ix.name} className="border-t border-border hover:bg-surface-2">
                <td className="px-3 py-1 font-mono">{ix.name}</td>
                <td className="px-3 py-1 font-mono">{keysText(ix.keys)}</td>
                <td className="px-3 py-1">
                  <span className="inline-flex gap-1">
                    {ix.unique && <Chip tone="primary">unique</Chip>}
                    {ix.expire_after_seconds !== null && <Chip tone="muted">{`TTL ${ix.expire_after_seconds}s`}</Chip>}
                  </span>
                </td>
                <td className="px-2 py-1 text-right">
                  {ix.name !== "_id_" && (
                    <button onClick={() => void drop(ix)} title={`Drop ${ix.name}`} aria-label={`Drop index ${ix.name}`} className="inline-flex h-5 w-5 items-center justify-center rounded text-muted hover:bg-surface-2 hover:text-danger">
                      <IconTrash size={12} />
                    </button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {rows === null && !error && <div className="p-3 text-xs text-muted">Loading…</div>}
      </div>
    </div>
  );
}
