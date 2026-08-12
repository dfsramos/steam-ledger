// A copy-pasteable activity log for reporting problems. Captures three
// generic interception points rather than hand-instrumenting every event
// handler across every screen: every backend call (api.js's invoke
// wrapper), every click/input (a single delegated listener in main.js), and
// screen/tab navigation (a small state-diff watcher, also wired in
// main.js). Kept out of state.js's plain data object on purpose — logging
// is infrastructure, not application state a screen renders from directly
// (the Log screen reads `entries()` instead).
//
// Never logs raw values from clicks/inputs (element identity only — id,
// class, dataset), and redacts known-sensitive backend-call args (Steam
// cookies) before they're ever appended — see `redactArgs`.

const MAX_ENTRIES = 500;
const entries = [];

// Backend commands whose args carry a raw secret that must never be
// persisted anywhere, including an in-memory log the user might copy into
// chat. Keyed by the exact `invoke()` arg name (see api.js's comment on
// Tauri's snake_case -> camelCase argument mapping).
const SENSITIVE_ARG_KEYS = new Set(["cookie"]);

function redactArgs(args) {
  if (!args || typeof args !== "object") return args;
  const out = {};
  for (const [key, value] of Object.entries(args)) {
    out[key] = SENSITIVE_ARG_KEYS.has(key)
      ? `<redacted, length=${typeof value === "string" ? value.length : "?"}>`
      : value;
  }
  return out;
}

// Keeps log lines short and copy-paste friendly — a full `list_items`
// result (hundreds of rows) dumped verbatim on every call would make the
// log useless to scroll/read, not more informative.
function summarize(value) {
  if (value === undefined) return "undefined";
  if (value === null) return "null";
  if (Array.isArray(value)) return `Array(${value.length})`;
  if (typeof value === "object") {
    const json = JSON.stringify(value);
    return json.length > 300 ? `${json.slice(0, 300)}…` : json;
  }
  return String(value);
}

function push(kind, message) {
  entries.push({ ts: new Date().toISOString(), kind, message });
  if (entries.length > MAX_ENTRIES) entries.shift();
}

export function logInteraction(kind, description) {
  if (!description) return;
  push(kind, description);
}

export function logNav(field, from, to) {
  push("nav", `${field}: ${from} -> ${to}`);
}

export function logBackendCall(command, args, outcome) {
  const argsText = args === undefined ? "" : summarize(redactArgs(args));
  const message = outcome.ok
    ? `${command}(${argsText}) -> ok (${outcome.durationMs}ms) ${summarize(outcome.result)}`
    : `${command}(${argsText}) -> FAILED (${outcome.durationMs}ms): ${outcome.error}`;
  push(outcome.ok ? "backend" : "error", message);
}

export function getEntries() {
  return entries;
}

export function clearLog() {
  entries.length = 0;
}

export function formatLog() {
  return entries
    .map((e) => `${e.ts}  ${e.kind.toUpperCase().padEnd(7)} ${e.message}`)
    .join("\n");
}
