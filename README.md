# 🏓 RubberMetrics Edge Workers

High-performance, mission-critical Cloudflare Workers powered by **Rust 2024** and **Wasm**. This workspace provides ultra-low latency services for the RubberMetrics ecosystem, optimized for the global edge.

## 🚀 Live Services

| Service | Endpoint | Purpose |
| :--- | :--- | :--- |
| **International ELO** | [https://international-elo.rubbermetrics.workers.dev](https://international-elo.rubbermetrics.workers.dev) | USATT Player Search (ID & Name) |
| **Search** | [https://search.rubbermetrics.workers.dev](https://search.rubbermetrics.workers.dev) | Product & Global Search |
| **Flight Curve** | [https://flightcurve.rubbermetrics.workers.dev](https://flightcurve.rubbermetrics.workers.dev) | Physics-based ball trajectory analysis |

---

## 🛠 Architecture & Performance

This project is built as a **Rust Cargo Workspace** for maximum code reuse and optimal binary sizing.

### ⚡ Key Performance Metrics
- **Sub-10µs Search:** Utilizing an **Inverted Index** and zero-allocation scoring paths.
- **Cold Start optimized:** RAM-based database deserialization in **~5ms**, well within Cloudflare's 10ms execution limit.
- **Lean Wasm Binaries:** Gzipped bundles are **< 550KB**, optimized via `wasm-opt` and `LTO`.

### 🧠 Advanced Algorithm (International ELO)
- **Hybrid Search:** Combines an $O(1)$ Hash-based ID lookup with an $O(k)$ Inverted Index for names.
- **Fuzzy Fallback:** Automatically falls back to **Jaro-Winkler** scoring for typos (e.g., `kank` -> `Kanak Jha`).
- **Memory Safety:** Uses boxed slices (`Box<[T]>`) and `OnceLock` for thread-safe, static global state without the overhead of `Arc` or `Mutex`.

---

## 💻 Development

### Prerequisites
- [Rust](https://www.rust-lang.org/) (Edition 2024)
- [Wrangler CLI](https://developers.cloudflare.com/workers/wrangler/install-upgrading/)
- `worker-build` (Cargo sub-command)

### Common Commands
```bash
# Test the entire workspace
cargo test --workspace --release

# Run a specific worker in dev mode
cd international-elo && npx wrangler dev

# Deploy to Cloudflare
cd international-elo && npx wrangler deploy
```

---

## 📦 Workspace Structure

- `shared-core/`: Common data models and utility logic.
- `international-elo/`: USATT player database and fuzzy search engine.
- `search/`: Global RubberMetrics search worker.
- `flightcurve/`: Trajectory simulation and analytics.

---

## 🔒 Security & Best Practices
- **CORS Management:** Strict `Vary: Origin` and `Access-Control-Allow-Origin` headers.
- **Cache-Control:** Edge-level caching (`max-age=3600`) for high-throughput query results.
- **Sanitized Inputs:** Strict character length constraints (3-32) and SQL-injection-safe numeric parsing.

---

© 2026 RubberMetrics. Mission Critical Edge Infrastructure.
