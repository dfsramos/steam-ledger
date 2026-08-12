import { beforeEach, describe, expect, it } from "vitest";
import { clearLog, formatLog, getEntries, logBackendCall, logInteraction, logNav } from "./applog.js";

beforeEach(() => {
  clearLog();
});

describe("logBackendCall", () => {
  it("redacts a cookie arg instead of logging it verbatim", () => {
    logBackendCall("save_steam_cookie_command", { cookie: "super-secret-session-value" }, {
      ok: true,
      durationMs: 12,
      result: null,
    });
    const [entry] = getEntries();
    expect(entry.message).not.toContain("super-secret-session-value");
    expect(entry.message).toContain("<redacted, length=26>");
  });

  it("summarizes an array result instead of dumping every row", () => {
    const items = Array.from({ length: 300 }, (_, i) => ({ id: i }));
    logBackendCall("list_items", undefined, { ok: true, durationMs: 5, result: items });
    const [entry] = getEntries();
    expect(entry.message).toContain("Array(300)");
    expect(entry.message).not.toContain('"id":299');
  });

  it("records a failed call as kind 'error' with the error message", () => {
    logBackendCall("refresh_item_price_command", { itemId: 42 }, {
      ok: false,
      durationMs: 900,
      error: "429 Too Many Requests",
    });
    const [entry] = getEntries();
    expect(entry.kind).toBe("error");
    expect(entry.message).toContain("429 Too Many Requests");
  });

  it("records a successful call as kind 'backend'", () => {
    logBackendCall("get_settings", undefined, { ok: true, durationMs: 3, result: { id: 1 } });
    expect(getEntries()[0].kind).toBe("backend");
  });
});

describe("logInteraction", () => {
  it("skips logging when there's no identifiable description", () => {
    logInteraction("click", null);
    expect(getEntries()).toHaveLength(0);
  });

  it("logs a click description verbatim", () => {
    logInteraction("click", "#sl-refresh");
    expect(getEntries()[0]).toMatchObject({ kind: "click", message: "#sl-refresh" });
  });
});

describe("logNav", () => {
  it("formats a screen transition", () => {
    logNav("screen", "portfolio", "settings");
    expect(getEntries()[0]).toMatchObject({ kind: "nav", message: "screen: portfolio -> settings" });
  });
});

describe("formatLog", () => {
  it("produces one readable line per entry, newest entries included", () => {
    logInteraction("click", "#a");
    logInteraction("click", "#b");
    const text = formatLog();
    expect(text.split("\n")).toHaveLength(2);
    expect(text).toContain("#a");
    expect(text).toContain("#b");
  });
});

describe("clearLog", () => {
  it("empties the log", () => {
    logInteraction("click", "#a");
    clearLog();
    expect(getEntries()).toHaveLength(0);
  });
});
