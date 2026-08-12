// Activity log screen: a copy-pasteable record of what the app has done
// this session — every backend call, click/input, and screen/tab change
// (see applog.js for what's captured and, importantly, what never is:
// no raw input values, no cookie contents).

import { state, notify } from "../state.js";
import { clearLog, formatLog, getEntries } from "../applog.js";

const KIND_COLOR = {
  backend: "#7d868f",
  error: "#e07a7a",
  click: "#5d9bd9",
  input: "#5d9bd9",
  change: "#5d9bd9",
  nav: "#d98a52",
};

function fromHTML(html) {
  const template = document.createElement("template");
  template.innerHTML = html.trim();
  return template.content;
}

export function render() {
  const entries = getEntries();

  const root = fromHTML(`
    <div style="flex:none; display:flex; align-items:center; justify-content:space-between; gap:12px; padding:14px 20px 12px; border-bottom:1px solid #1b1f23;">
      <div style="min-width:0;">
        <div style="font-size:20.5px; font-weight:600; letter-spacing:-0.01em;">Log</div>
        <div style="font-size:14px; color:#7d868f; margin-top:2px;">Every backend call, click, and screen change this session — copy and paste it when reporting an issue. Never records typed values or cookies.</div>
      </div>
      <div style="display:flex; gap:8px; flex:none;">
        <div id="sl-log-copy" class="sl-btn-ghost" style="padding:6px 11px; border:1px solid #2b3137; border-radius:4px; font-size:14.5px; line-height:1.2; white-space:nowrap; color:#b9c1c7; cursor:pointer; background:#15181b;">${state.logCopyLabel ?? "Copy to clipboard"}</div>
        <div id="sl-log-clear" class="sl-btn-ghost" style="padding:6px 11px; border:1px solid #4a2a2a; border-radius:4px; font-size:14.5px; line-height:1.2; white-space:nowrap; color:#e08585; cursor:pointer; background:#1a1213;">Clear</div>
      </div>
    </div>

    <div data-scroll-key="log-content" style="flex:1; overflow-y:auto; min-height:0; padding:12px 20px; font-family:'IBM Plex Mono', monospace; font-size:12.5px; line-height:1.7;">
      ${
        entries.length === 0
          ? `<div style="color:#5d666d;">Nothing logged yet — use the app and this will fill in as you go.</div>`
          : entries
              .slice()
              .reverse()
              .map((e) => {
                const time = e.ts.slice(11, 23);
                const color = KIND_COLOR[e.kind] ?? "#98a1a8";
                return `<div style="display:flex; gap:10px; padding:2px 0; border-bottom:1px solid #14171a;">
                  <div style="color:#5d666d; flex:none; white-space:nowrap;">${time}</div>
                  <div style="color:${color}; flex:none; width:56px; text-transform:uppercase; white-space:nowrap;">${e.kind}</div>
                  <div style="color:#c9d0d5; overflow-wrap:anywhere;">${e.message}</div>
                </div>`;
              })
              .join("")
      }
    </div>
  `);

  root.querySelector("#sl-log-copy").addEventListener("click", () => {
    const text = formatLog();
    navigator.clipboard.writeText(text).then(
      () => {
        state.logCopyLabel = "Copied!";
        notify();
        setTimeout(() => {
          state.logCopyLabel = null;
          notify();
        }, 1500);
      },
      (err) => {
        console.error("Clipboard write failed, falling back to select-all", err);
        // Some webview/security contexts block the async Clipboard API —
        // fall back to a selectable textarea so the user can Ctrl+C
        // themselves rather than the button silently doing nothing.
        const textarea = document.createElement("textarea");
        textarea.value = text;
        textarea.style.position = "fixed";
        textarea.style.opacity = "0";
        document.body.appendChild(textarea);
        textarea.focus();
        textarea.select();
        document.execCommand("copy");
        document.body.removeChild(textarea);
      },
    );
  });

  root.querySelector("#sl-log-clear").addEventListener("click", () => {
    clearLog();
    notify();
  });

  return root;
}
