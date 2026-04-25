# LocalMind

Run open-source LLMs entirely on your device. Tauri 2 + React + llama.cpp.

Features: chat with streaming, HuggingFace marketplace, auto hardware detection (Metal / CUDA / Vulkan), RAG over your documents, vision models (LLaVA), voice input/TTS, image generation via stable-diffusion.cpp, and a LAN web UI for phones/tablets on the same Wi-Fi.

## Prerequisites

- **Node.js** 18+ and **npm**
- **Rust** stable toolchain ([install via rustup](https://rustup.rs))
- **Xcode Command Line Tools** (macOS) — `xcode-select --install`
- **WebView2** (Windows) — preinstalled on Win 11
- **webkit2gtk + build-essential** (Linux)

llama.cpp itself is downloaded automatically on first model load — no manual setup.

## Run in development

```bash
cd localmind
npm install
npm run tauri dev
```

The Tauri window opens at `http://127.0.0.1:1420` (Vite) wrapped in a native shell. First boot detects your hardware and writes data to:

- macOS: `~/Library/Application Support/LocalMind/`
- Linux: `~/.local/share/LocalMind/`
- Windows: `%APPDATA%\LocalMind\`

## Build a release bundle

```bash
npm run tauri build
```

Outputs platform-native installers under `src-tauri/target/release/bundle/` (.dmg, .msi, .deb, .AppImage).

## First-run flow

1. Open **Marketplace**, search for a model (e.g. `qwen2.5-7b-instruct GGUF`), click **get**.
2. The llama.cpp engine downloads automatically the first time you load a model (~30-100 MB).
3. Open **Chat**, pick the model from the top-right dropdown, and send a message.

### Vision (LLaVA) models

A vision model needs **two files** from the same repo:

- The chat model — e.g. `llava-v1.6-mistral-7b.Q4_K_M.gguf`
- A matching projector — e.g. `mmproj-model-f16.gguf`

Both must come from the same author/repo or the projector won't align with the model's vision tower. The marketplace doesn't auto-pair them — download both manually from the file list.

### LAN access from phone/tablet

While `tauri dev` (or the built app) is running, look at **Settings → Access from other devices** for a `http://<your-lan-ip>:3939` URL. Open it on any device on the same Wi-Fi for the full UI without installing anything. Disabled by default outside dev — embedded server only listens on the LAN, never on `0.0.0.0` publicly.

## Project layout

```
localmind/
├── src/                  # React frontend (Vite)
│   ├── pages/            # Chat, Marketplace, Models, Knowledge, ImageGen, Settings
│   ├── components/       # Sidebar, HardwareBadge, etc.
│   └── lib/              # store (Zustand), api, types
├── src-tauri/
│   ├── src/              # Rust backend
│   │   ├── llama.rs      # spawns llama-server children
│   │   ├── binaries.rs   # downloads/extracts llama.cpp + sd
│   │   ├── models.rs     # HF search, download, listing
│   │   ├── rag.rs        # document ingest + embedding search
│   │   ├── sd.rs         # stable-diffusion.cpp orchestration
│   │   └── server.rs     # Axum LAN server
│   └── Cargo.toml
└── package.json
```

## Troubleshooting

- **"image input is not supported"** — your vision model is missing its mmproj projector. See *Vision models* above.
- **Model output loops on a fragment** — already handled via repeat/frequency penalties in `streamChat`. If still bad, try a different quant.
- **`llama-server did not become ready`** — usually a port conflict. Stop the model from My-models and try again; orphaned servers on 8181/8182 are auto-killed before respawn.
- **Black screen on macOS** — make sure `devUrl` is `http://127.0.0.1:1420` (not `localhost:1420`) in `src-tauri/tauri.conf.json`.

## License

MIT.
