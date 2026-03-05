# Chapter 4: User Guide

User-facing features: keyboard shortcuts, filtering, mouse navigation, fullscreen modes, and accessibility.

## Keyboard Controls

Press `?` in web/Tauri to show the shortcuts overlay.

### Navigation

| Key | Action |
|-----|--------|
| `Tab` / `Shift+Tab` | Switch panes (Blocks -> Txs -> Details -> Blocks) |
| `Up/Down` or `j/k` | Navigate lists or scroll details |
| `Left/Right` or `h/l` | Jump to top / paginate down 12 items |
| `PgUp/PgDn` | Page scroll (20 lines) |
| `Home` | Blocks pane: return to auto-follow; others: jump to top |
| `End` | Jump to bottom |
| `Enter` | Select transaction |
| `Space` | Toggle fullscreen details (web/Tauri) |

### Filtering & Search

| Key | Action |
|-----|--------|
| `/` or `f` | Enter filter mode (real-time filtering) |
| `Ctrl+F` | Open history search (SQLite-backed, native only) |
| `Esc` | Close overlay / clear filter / exit mode (priority order) |

### Bookmarks (Jump Marks)

| Key | Action |
|-----|--------|
| `m` | Set mark at current position |
| `Ctrl+P` | Pin/unpin current position (persistent) |
| `Shift+M` | Open marks overlay |
| `'` (apostrophe) | Quick jump (type label character) |
| `[` / `]` | Jump to previous/next mark |
| `d` | Delete mark (in marks overlay) |

### Other

| Key | Action |
|-----|--------|
| `Ctrl+O` | Cycle FPS (toggles through configured choices) |
| `Ctrl+D` | Toggle debug panel |
| `Ctrl+U` | Toggle owned-accounts-only filter |
| `c` | Copy pane content to clipboard |
| `q` or `Ctrl+C` | Quit |

## Filter System

Real-time transaction filtering with query grammar:

```
acct:alice.near       # Match signer OR receiver
signer:bob.near       # Match signer only
receiver:contract     # Match receiver only
action:FunctionCall   # Match action type
method:ft_transfer    # Match method name
raw:some_text         # Search in raw JSON
freetext              # Match anywhere
```

**Logic**: AND between different field types, OR within each field type.

**Examples**:
- `acct:myaccount.near` -- all transactions involving your account
- `method:ft_transfer` -- fungible token transfers
- `action:FunctionCall method:swap` -- swap function calls
- `raw:error` -- transactions with errors in JSON

## Mouse Navigation (Web/Tauri)

- **Click** -- focus pane and select row
- **Double-click Details** -- toggle fullscreen overlay
- **Wheel scroll** -- navigate focused pane
- **Hover** -- pointer cursor and subtle highlight on hoverable rows

## Text Selection & Copying

### Native TUI

NEARx captures the mouse for pane navigation. To select text:

- **macOS iTerm2**: hold `Option/Alt`
- **macOS Terminal.app**: hold `Fn`
- **Linux**: hold `Shift`
- **Windows**: hold `Shift`

### Web/Tauri

Text selection works natively -- click and drag.

### Copy Shortcut (`c`)

Content copied depends on focused pane:
- **Blocks**: all transactions in selected block
- **Transactions**: human-readable view + raw JSON
- **Details**: full JSON content

Visual feedback: border flashes green briefly.

## Fullscreen Dual-Mode Navigation

Press `Space` to enter fullscreen. `Tab` toggles between two modes:

### Scroll Mode (default)
Arrow keys scroll the JSON content. Ideal for browsing large block/transaction JSON.

### Navigate Mode
Arrow keys navigate underlying Blocks/Txs rows. JSON updates live as you browse. Ideal for comparing adjacent blocks.

### Fullscreen Keys

| Key | Action |
|-----|--------|
| `Space` | Enter/exit fullscreen |
| `Tab` | Toggle Scroll <-> Navigate |
| `Up/Down` or `j/k` | Scroll or navigate (mode-dependent) |
| `PgUp/PgDn` | Page scroll (20 lines) |
| `Esc` | Exit fullscreen |
| `c` | Copy JSON content |

### Block Context Window

In fullscreen, the app eagerly fetches +/-50 blocks around the selected height using archival RPC. Loading indicator shows while fetching.

## History Search (Native Only)

Press `Ctrl+F` to search through all previously seen transactions. Uses same filter syntax as real-time filtering. Results stored in local SQLite database.

## Owned Accounts Filter

Press `Ctrl+U` to toggle showing only transactions involving your accounts. Accounts auto-discovered from `~/.near-credentials`. Visual indicator in footer when active.

## Accessibility

### Focus Indicators

Four simultaneous visual changes on pane focus:
1. Border color: gray -> bright yellow `#ffcc00`
2. Border style: 1px -> 2px bold + inner glow
3. Background: subtle "lift" effect
4. Border shadow: outer glow + inner highlight

### WCAG AA Compliance

All color ratios meet AA minimum:
- Text on panel: 12.4:1 (AAA)
- Focused border: 9.8-11.5:1 (AAA)
- Unfocused border: 3.08:1 (AA)
- Selection highlight: >=4.5:1

Automated WCAG tests in `src/theme.rs` run in CI.

### Keyboard-Only Operation

Entire app navigable without mouse. Tab focuses panes, arrows navigate lists, Space/Enter activate items.

### Browser Compatibility

Tested: Chrome/Edge, Firefox, Safari (latest). Zoom support: 100-200%. High contrast mode supported via forced-colors media query.
