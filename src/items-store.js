// Shared item-loading logic for anything that reads state.items (the
// portfolio screen's table, the shell's vault-value box). No-ops while a
// load is already in flight or state.items already holds a value; the
// in-flight flags must be reset once the fetch settles (success or
// failure), not just on failure — otherwise setting state.items = undefined
// after a mutation (remove/sell/wipe/import, all of which do this to force
// a refresh) never actually re-fetches past the very first load, and the
// list just renders empty from then on. notify() drives the re-render once
// the promise resolves.

import { state, notify } from "./state.js";
import * as api from "./api.js";

let loadStarted = false;

export function ensureItemsLoaded() {
  if (state.items !== undefined || loadStarted) return;
  loadStarted = true;
  api
    .listItems()
    .then((items) => {
      loadStarted = false;
      state.items = items;
      notify();
    })
    .catch((err) => {
      console.error("Failed to load items", err);
      loadStarted = false;
      state.items = [];
      notify();
    });
}

let settingsLoadStarted = false;

export function ensureSettingsLoaded() {
  if (state.settings !== undefined || settingsLoadStarted) return;
  settingsLoadStarted = true;
  api
    .getSettings()
    .then((settings) => {
      settingsLoadStarted = false;
      state.settings = settings;
      notify();
    })
    .catch((err) => {
      console.error("Failed to load settings", err);
      settingsLoadStarted = false;
    });
}

let backfillAttempted = false;

// One-shot, best-effort: resolves game_name for any items persisted before
// that column existed (rows imported by an older build show "appid N" until
// this runs). Runs once per app session, not on every render — the backend
// already no-ops quickly when there's nothing to backfill, but there's no
// reason to hit Steam's API repeatedly for a fact that doesn't change. Only
// forces a reload of state.items when rows actually changed.
export function ensureGameNamesBackfilled() {
  if (backfillAttempted) return;
  backfillAttempted = true;
  api
    .backfillGameNames()
    .then((updated) => {
      if (updated > 0) {
        state.items = undefined;
        ensureItemsLoaded();
      }
    })
    .catch((err) => console.error("Failed to backfill game names", err));
}

let selectedLoadingId = null;

// Lazily fetches an item + its price history whenever state.selId changes
// from the last-loaded id. No-ops if id is null or already loaded/in-flight.
export function ensureItemAndHistoryLoaded(id) {
  if (id == null) return;
  if (state.selItem?.id === id) return;
  if (selectedLoadingId === id) return;

  selectedLoadingId = id;
  Promise.all([api.getItem(id), api.getPriceHistory(id)])
    .then(([item, history]) => {
      selectedLoadingId = null;
      state.selItem = item;
      state.selHistory = history;
      notify();
    })
    .catch((err) => {
      console.error("Failed to load item detail", err);
      selectedLoadingId = null;
      notify();
    });
}
