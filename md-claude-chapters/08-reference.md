# Chapter 8: Reference

This chapter contains technical reference information including dependencies, performance characteristics, troubleshooting, and advanced features.

## Dependencies

### Quad-Mode Dependency Strategy

NEARx uses **feature flags** and **optional dependencies** with strict `dep:` mappings to prevent cross-contamination:

```toml
[features]
default = []  # No defaults - explicit feature selection required
native = [
    # Native UI/IO (ALL optional with dep: mapping)
    "dep:crossterm", "dep:copypasta", "dep:rusqlite", "dep:notify",
    # WebSocket support
    "dep:tokio-tungstenite", "dep:tungstenite", "dep:futures-util",
    # NEAR SDK crates (have C dependencies)
    "dep:near-primitives", "dep:near-crypto", "dep:near-jsonrpc-client",
    "dep:near-jsonrpc-primitives", "dep:near-account-id", "dep:near-gas", "dep:near-token",
    # Tokio features
    "tokio/rt-multi-thread", "tokio/macros", "tokio/time", "tokio/signal",
    "tokio/fs", "tokio/io-util",
]

dom-web = [
    # Pure DOM dependencies (no egui!)
    "dep:wasm-bindgen",
    "dep:wasm-bindgen-futures",
    "dep:js-sys",
    "dep:web-sys",
    # ... other web-specific deps
]
```

### WASM Compatibility Challenges & Solutions

**Challenge 1: NEAR SDK C Dependencies**

The official NEAR SDK crates depend on native C libraries:
- `zstd-sys` - Compression library (C code)
- `secp256k1-sys` - Cryptographic primitives (C code)
- `ed25519-dalek` with native features

**Solution:**
- Made all NEAR crates **optional** dependencies
- Use conditional compilation `#[cfg(feature = "native")]`
- Created web-compatible stub implementations

**Challenge 2: Tokio Runtime**

Tokio's default features include `net` which uses `mio` (not WASM-compatible).

**Solution:**
```toml
[target.'cfg(target_arch = "wasm32")'.dependencies]
tokio = { version = "1", default-features = false, features = ["sync", "macros", "time"] }
```

**Challenge 3: Platform-Specific Features**

Features like clipboard, SQLite, file watching are native-only.

**Solution:**
- Platform abstraction layer (`src/platform/`)
- Separate implementations for native vs web
- 4-tier clipboard fallback chain for web

## Performance Characteristics

- **Memory**: ~10MB baseline + (100 blocks × avg tx size)
- **CPU**: <5% on modern hardware at 30 FPS
- **Disk I/O**: WAL mode enables concurrent reads during writes
- **Network**: Configurable polling interval + catch-up limits

## Troubleshooting

### Connection Issues

**Connection refused with SOURCE=ws**:
- Ensure Node WebSocket server is running on port 63736
- Check WS_URL matches your Node configuration

**RPC timeouts**:
```bash
RPC_TIMEOUT_MS=15000 POLL_CHUNK_CONCURRENCY=2 cargo run --bin nearx --features native
```

### Performance Issues

**High CPU usage**:
```bash
RENDER_FPS=20 KEEP_BLOCKS=50 cargo run --bin nearx --features native
```

**Search not finding results**:
- Ensure SQLite history has been populated (run for a few minutes first)
- Check query syntax matches filter grammar

### Web Build Errors

1. **Build fails with zstd-sys/secp256k1-sys errors**:
   - **Cause**: Default features include native NEAR SDK crates
   - **Fix**: Use `--no-default-features --features dom-web` flags

2. **Runtime panic: "time not implemented on this platform"**:
   - **Cause**: Some `std::time` usage not WASM-compatible
   - **Status**: Known issue, active development
   - **Workaround**: Affects specific time-based features

3. **Connection refused errors in browser console**:
   - **Cause**: Web app trying to connect to localhost proxy
   - **Fix**: Configure RPC endpoint properly

## Known Limitations

### Web Mode Limitations
- ⚠️ **Time-based features**: Some chrono usage not fully WASM-compatible
- ⚠️ **No SQLite**: History and marks are in-memory only
- ⚠️ **RPC only**: WebSocket mode not available
- ⚠️ **No file access**: Credential watching disabled
- ✅ **Core functionality**: Block viewing, filtering, and navigation work perfectly

### Current Known Issues
1. **Tab key not working** in Web/Tauri - related to egui remnants
2. **Performance regression** - possibly due to outdated UI flags
3. **Incorrect target paths** in documentation and CI scripts

## Fullscreen Dual-Mode Navigation

Press Space in any pane to enter fullscreen. Tab toggles between two interaction modes:

### Modes

**Scroll Mode** (default):
- Arrow keys: Scroll the JSON content
- Ideal for: Browsing massive raw block/transaction JSON

**Navigate Mode** (Tab to activate):
- Arrow keys: Navigate underlying Blocks/Txs rows
- JSON updates live as you browse
- Ideal for: Comparing adjacent blocks/transactions

### Keyboard Shortcuts (Fullscreen)
- `Space`: Enter/exit fullscreen
- `Tab`: Toggle Scroll ↔ Navigate modes
- `↑↓/jk`: Scroll JSON (Scroll mode) or navigate rows (Navigate mode)
- `PgUp/PgDn`: Page scroll (20 lines)
- `Esc`: Exit fullscreen
- `c`: Copy JSON content

### Visual Indicators
- Title bar shows current mode: `"↕ Scroll"` or `"↑↓ Navigate"`
- Content type: `"Block Raw JSON"` | `"Transaction Raw JSON"` | `"Parsed Details"`
- Hint: `"Tab=switch • c=copy • Space=exit"`

### ±50 Block Context Window

When viewing block raw JSON in fullscreen:
- App eagerly fetches ±50 blocks around the selected height
- Uses archival RPC for historical blocks beyond live buffer
- Enables seamless navigation through 100-block windows
- Loading indicator: `"⏳ Loading block #..."`

**Configuration**:
```bash
# Enable archival RPC (optional, but recommended for deep history)
ARCHIVAL_RPC_URL=https://archival-rpc.mainnet.fastnear.com/
```

### Safety Features

**Streaming JSON Truncation**:
- Raw block JSON truncated at 100KB to prevent UI freezing
- Footer shows: `"... (truncated - N bytes total, showing first 100 KB)"`
- Prevents stack overflow on blocks with 1000+ transactions

**WASM Archival Support**:
- Web and Tauri targets have full archival fetch parity with TUI
- Uses browser Fetch API (non-blocking, CORS-compatible)
- Automatic retry with exponential backoff

## Future Enhancements

- **FTS5 Support**: Full-text search upgrade when SQLite has FTS5
- **Plugin System**: Currently disabled due to lifetime issues
- **Nested Delegate Actions**: Support for deeply nested DelegateAction chains
- **Copy Structure Parity**: Implement csli-dashboard's pane-specific copy formats
- **Fix Tab key handling**: Remove egui remnants from DOM builds
- **Performance optimization**: Address current sluggishness issues
- **Fix CI/release paths**: Correct target directory references

## Project Notes

### Arbitrage Engine (Moved)
The arbitrage scanning engine has been moved to a separate workspace. The `ref-arb-scanner` crate is now an independent workspace member located in the `ref-arb-scanner/` directory.

To use the arbitrage scanner:
```bash
# Navigate to the scanner directory
cd ref-arb-scanner

# Build and run
cargo run --release
```

## Credits

Built with ❤️ using Ratatui, Tokio, and Rust. Designed for NEAR Protocol monitoring.

---

Return to:
- [Chapter 1: Getting Started](01-getting-started.md)
- [Main Documentation Hub](../CLAUDE.md)