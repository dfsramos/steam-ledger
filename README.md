# Steam Ledger

A local-first desktop app for tracking your Steam game skin/item portfolio: what you paid, what it's worth now, and your realised profit and loss over time.

[**Download for Windows**](https://github.com/dfsramos/steam-ledger/releases/latest) · Built with [Tauri](https://tauri.app/)

## What it does

- **Portfolio dashboard** — every item you own, current market value, P/L per item and overall, sortable and filterable by game.
- **Real Steam import** — pulls your actual Community Market purchase history and current inventory, reconciles the two (so items you've since resold don't show up as still-held), and fills in prices from your in-game Store purchase history and multi-item pack receipts too.
- **One-click account connect** — a "Connect Steam Account" button opens a real Steam login window inside the app and picks up your session automatically; no digging through browser devtools for cookie values.
- **Live market prices** — refresh current values from the Steam Community Market on demand, per item or for your whole portfolio.
- **CSV/JSON export** and a local backup path setting.
- **Local-first** — everything lives in a SQLite file on your own machine (`~/.steamledger/vault.db`). No account, no cloud sync, no telemetry. Your Steam session cookie is stored via your OS's native credential store (Windows Credential Manager) and is only ever sent directly to Steam's own servers.

## Download

Windows builds are published as [GitHub Releases](https://github.com/dfsramos/steam-ledger/releases) — grab the latest installer from there. Other platforms aren't built or tested yet.

## Building from source

Prerequisites: [Node.js](https://nodejs.org/) 18+, [Rust](https://rustup.rs/) (via rustup), and the [Tauri v2 prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS.

```sh
npm install
npm run tauri dev    # run locally
npm run tauri build  # produce an installer for your current platform
```

Rust tests: `cd src-tauri && cargo test`. Frontend tests: `npx vitest run`.

## How the Steam import works

Steam doesn't expose an official API for market purchase history, so this app authenticates using the same session cookie your browser already has after logging in — nothing more privileged than what your browser can already do. Three Steam domains (`steamcommunity.com`, `store.steampowered.com`, `help.steampowered.com`) issue independent session cookies; "Connect Steam Account" walks you through logging in once and collects all three automatically.

## License

[MIT](LICENSE)
