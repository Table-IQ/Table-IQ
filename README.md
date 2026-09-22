# Tauri + React + Typescript

This template should help get you started developing with Tauri, React and Typescript in Vite.

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

## Real-app QA (browser → real Rust → real DB)

Run the real React UI in a normal browser against the real Rust commands (no browser mock).
`TIQ_CONFIG_DIR` keeps QA connections, history and settings out of your real config dir;
passwords still go to the OS keychain, so use throwaway connection ids.

```sh
cd src-tauri && TIQ_CONFIG_DIR=/tmp/tiq-qa cargo run --features ipc-bridge --example ipc_bridge
VITE_IPC_BRIDGE=1 pnpm dev   # open http://localhost:1420
```

Commands can also be called directly: `curl -s -XPOST localhost:1430/invoke/list_connections -d '{}'`.
