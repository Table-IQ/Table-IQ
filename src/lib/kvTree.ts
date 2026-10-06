/** The key sidebar's model: SCAN pages merged into one list, folded into a `:` folder tree whose
 *  counts are of the keys loaded so far (a SCAN never knows the total for a pattern). */
import type { KeyInfo, KvItem } from "./commands";

export const SEP = ":";

export interface KeyNode {
  /** The segment shown. */
  name: string;
  /** Unique id: a folder is `prefix:` (trailing separator), a key is the full key. */
  path: string;
  folder: boolean;
  /** Keys loaded at or under this node (1 for a key). */
  count: number;
  children: KeyNode[];
  info?: KeyInfo;
}

/** Add `incoming` to `existing`, dropping repeats by key (SCAN may return a key twice); a repeat refreshes its type / TTL. */
export function mergeKeys(existing: KeyInfo[], incoming: KeyInfo[]): KeyInfo[] {
  const at = new Map(existing.map((k, i) => [k.key, i]));
  const out = [...existing];
  for (const k of incoming) {
    const i = at.get(k.key);
    if (i === undefined) {
      at.set(k.key, out.length);
      out.push(k);
    } else out[i] = k;
  }
  return out;
}

const byName = (a: KeyNode, b: KeyNode) => a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" }) || (a.name < b.name ? -1 : a.name > b.name ? 1 : 0);

/** Fold keys into folders on `:`. `user:1:name` is key `name` in folder `user:1:` in folder `user:`. A name
 *  can be both a key and a folder (`a`, `a:b`): two nodes. Folders sort before keys; numbers sort by value. */
export function buildKeyTree(keys: KeyInfo[]): KeyNode[] {
  const root: KeyNode = { name: "", path: "", folder: true, count: 0, children: [] };
  const folders = new Map<string, KeyNode>([["", root]]);
  for (const info of keys) {
    const parts = info.key.split(SEP);
    let parent = root;
    let prefix = "";
    for (const seg of parts.slice(0, -1)) {
      prefix += seg + SEP;
      let f = folders.get(prefix);
      if (!f) {
        f = { name: seg, path: prefix, folder: true, count: 0, children: [] };
        folders.set(prefix, f);
        parent.children.push(f);
      }
      f.count++;
      parent = f;
    }
    parent.children.push({ name: parts[parts.length - 1], path: info.key, folder: false, count: 1, children: [], info });
    root.count++;
  }
  const sort = (n: KeyNode) => {
    n.children.sort((a, b) => (a.folder === b.folder ? byName(a, b) : a.folder ? -1 : 1));
    n.children.forEach(sort);
  };
  sort(root);
  return root.children;
}

export interface TreeRow {
  node: KeyNode;
  depth: number;
}

/** The visible rows: a folder's children only while its path is in `open`. */
export function flattenTree(nodes: KeyNode[], open: ReadonlySet<string>, depth = 0): TreeRow[] {
  return nodes.flatMap((node) => [{ node, depth }, ...(node.folder && open.has(node.path) ? flattenTree(node.children, open, depth + 1) : [])]);
}

/** `1,234` / `1,234+` — a loaded count is a floor while the scan has pages left. */
export function loadedLabel(n: number, more: boolean): string {
  return `${n.toLocaleString("en-US")}${more ? "+" : ""}`;
}

/** What identifies an item of a key's value across pages: a list's index, a zset / set member, a hash field, a
 *  stream entry id. Never the value text: a list may hold the same value many times. */
export function itemKey(kind: string, i: KvItem): string {
  const b = kind === "hash" ? i.field : kind === "set" || kind === "zset" ? i.value : undefined;
  return kind === "list" ? `i:${i.index}` : kind === "stream" ? `id:${i.id}` : `m:${b?.b64 ?? b?.text}`;
}

/** Append a page of items, dropping those already loaded (SSCAN / HSCAN may repeat an element). */
export function mergeItems(kind: string, cur: KvItem[], more: KvItem[]): KvItem[] {
  const seen = new Set(cur.map((i) => itemKey(kind, i)));
  return [...cur, ...more.filter((i) => !seen.has(itemKey(kind, i)))];
}
