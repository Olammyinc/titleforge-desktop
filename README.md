# TitleForge Desktop

Offline title generator for Windows, Mac, and Linux. Generates titles for books, articles, YouTube videos, songs, podcasts, and more — powered by a curated database of 480+ templates and 700+ curated titles. No internet required, no monthly fees.

## Prerequisites

- Rust (https://rustup.rs)
- Node.js 18+ (for Tauri CLI)

## Setup

```bash
git clone https://github.com/Olammyinc/titleforge-desktop.git
cd titleforge-desktop
npm install
```

The seed database is included in `seed-data.json`. It's automatically imported on first launch.

## Development

```bash
npm run dev
```

## Build

```bash
npm run build
```

Output will be in `src-tauri/target/release/`.

## License

Private. All rights reserved.
