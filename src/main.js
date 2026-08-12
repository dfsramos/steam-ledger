// Entry point: wires the reactive core (state.js) to the DOM. Every
// `notify()` call triggers a full re-render of the shell + whichever screen
// matches `state.screen` — simple full re-render, no diffing, which is fine
// at this app's scale.

import { state, subscribe, notify } from "./state.js";
import { render as renderShell } from "./screens/shell.js";
import { render as renderPortfolio } from "./screens/portfolio.js";
import { render as renderAdd } from "./screens/add.js";
import { render as renderDetail } from "./screens/detail.js";
import { render as renderSettings } from "./screens/settings.js";
import { render as renderLog } from "./screens/log.js";
import { logInteraction, logNav } from "./applog.js";

const SCREEN_RENDERERS = {
  portfolio: renderPortfolio,
  add: renderAdd,
  detail: renderDetail,
  settings: renderSettings,
  log: renderLog,
};

const SCREEN_KEYS = {
  1: "portfolio",
  2: "add",
  3: "detail",
  4: "settings",
  5: "log",
};

// Every scrollable region a screen owns (a table body, a log panel, a
// review list, ...) marks itself with `data-scroll-key="<unique-in-screen>"`
// — captured before teardown and restored after rebuild, the same way focus
// is preserved below. Without this, any notify() (which fires on nearly
// every click) snaps every scrollable panel back to the top, since a fresh
// DOM node always starts at scrollTop 0.
function captureScrollPositions() {
  const positions = {};
  document.querySelectorAll("#app [data-scroll-key]").forEach((el) => {
    positions[el.dataset.scrollKey] = el.scrollTop;
  });
  return positions;
}

function restoreScrollPositions(positions) {
  document.querySelectorAll("#app [data-scroll-key]").forEach((el) => {
    if (el.dataset.scrollKey in positions) {
      el.scrollTop = positions[el.dataset.scrollKey];
    }
  });
}

function render() {
  const app = document.getElementById("app");

  // Full teardown/rebuild on every notify() loses focus and cursor position
  // on whatever input the user was typing in (e.g. the portfolio search
  // box) — capture it here and restore it after rebuilding, since inputs
  // are re-created as new DOM nodes each render.
  const active = document.activeElement;
  const focusedId = active && active.id ? active.id : null;
  const selectionStart = active && "selectionStart" in active ? active.selectionStart : null;
  const selectionEnd = active && "selectionEnd" in active ? active.selectionEnd : null;

  const scrollPositions = captureScrollPositions();

  app.innerHTML = "";

  const shell = renderShell();
  app.appendChild(shell);

  const screenRoot = shell.querySelector("#screen-root");
  const renderScreen = SCREEN_RENDERERS[state.screen] ?? renderPortfolio;
  screenRoot.appendChild(renderScreen());

  if (focusedId) {
    const toRefocus = document.getElementById(focusedId);
    if (toRefocus) {
      toRefocus.focus();
      if (selectionStart !== null && "setSelectionRange" in toRefocus) {
        toRefocus.setSelectionRange(selectionStart, selectionEnd);
      }
    }
  }

  restoreScrollPositions(scrollPositions);
}

subscribe(render);

// Activity logging (see applog.js and screens/log.js): two generic
// interception points cover "every click/input" and "every screen/tab
// change" without a log call at each of the dozens of individual event
// handlers across every screen file. Never captures typed values — element
// identity only (id/class/dataset) — so a search query or a pasted cookie
// value can never end up in a log the user might copy into chat.
function describeInteractionTarget(target) {
  const el =
    target instanceof Element
      ? target.closest("[id], [data-nav], [data-id], [data-row-id], [data-filter], [data-index]")
      : null;
  if (!el) return null;
  if (el.id) return `#${el.id}`;
  const cls = typeof el.className === "string" ? el.className.split(" ")[0] : null;
  const dataBits = Object.entries(el.dataset)
    .map(([k, v]) => `${k}=${v}`)
    .join(",");
  const label = cls ? `.${cls}` : el.tagName.toLowerCase();
  return dataBits ? `${label}[${dataBits}]` : label;
}

document.addEventListener(
  "click",
  (event) => logInteraction("click", describeInteractionTarget(event.target)),
  true,
);

document.addEventListener(
  "input",
  (event) => logInteraction("input", describeInteractionTarget(event.target)),
  true,
);

// `<select>` and checkbox/date inputs commit via "change", not reliably via
// "input" across browsers/webviews — covered separately so those actions
// (e.g. the portfolio game filter, a Steam-import row's include checkbox)
// aren't silently missing from the log.
document.addEventListener(
  "change",
  (event) => logInteraction("change", describeInteractionTarget(event.target)),
  true,
);

// Watches a small, deliberately narrow set of navigation-shaped fields —
// not a generic deep-diff of the whole state object, which would be noisy
// (dozens of unrelated fields change on nearly every notify()) and isn't
// needed since clicks are already logged separately above.
let lastScreen = state.screen;
subscribe(() => {
  if (state.screen !== lastScreen) {
    logNav("screen", lastScreen, state.screen);
    lastScreen = state.screen;
  }
});

window.addEventListener("keydown", (event) => {
  const target = event.target;
  const isTyping =
    target instanceof HTMLElement &&
    (target.tagName === "INPUT" || target.tagName === "TEXTAREA");

  if (isTyping) {
    return;
  }

  if (event.key in SCREEN_KEYS) {
    state.screen = SCREEN_KEYS[event.key];
    notify();
    return;
  }

  if (event.key === "/") {
    event.preventDefault();
    const firstInput = document.querySelector("#app input, #app textarea");
    if (firstInput) {
      firstInput.focus();
    }
  }
});

notify();
