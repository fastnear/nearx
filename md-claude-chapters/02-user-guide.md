# Chapter 2: User Guide

This guide covers all user-facing features of NEARx, including keyboard controls, filtering, mouse navigation, and accessibility features.

## Keyboard Controls

**Web/Tauri Help**: Press `?` to show keyboard shortcuts modal

### Navigation
- `Tab` / `Shift+Tab` - Switch panes (circular: Blocks → Txs → Details → Blocks)
- `↑ / ↓` or `j / k` - Navigate lists or scroll details (pane-specific, Vim-style)
- `← / →` or `h / l` - Jump to top / Paginate down 12 items (Vim-style horizontal)
- `PgUp / PgDn` - Page scroll (20 lines)
- `Home` - In blocks pane: return to auto-follow mode; Other panes: jump to top
- `End` - Jump to bottom
- `Enter` - Select transaction
- `Space` - Toggle fullscreen details (Web/Tauri)

### Filtering & Search
- `/` or `f` - Enter filter mode (real-time filtering)
- `Ctrl+F` - Open history search (SQLite-backed, native only)
- `Esc` - Close fullscreen overlay, clear filter, or exit mode (priority order)

### Mouse Navigation (Web/Tauri)
- **Click** - Focus pane and select row (Blocks/Tx) or focus Details
- **Double-click Details** - Toggle fullscreen overlay
- **Wheel scroll** - Navigate through focused pane (Blocks/Tx lists or Details scrolling)
- **Hover effects**: Rows show pointer cursor and subtle highlight when hoverable
- **Text selection**: Native browser text selection works in Details pane

### Bookmarks (Jump Marks)
- `m` - Set mark at current position (auto-labeled)
- `Ctrl+P` - Pin/unpin current position (persistent across sessions)
- `Shift+M` - Open marks overlay (list all marks)
- `'` (apostrophe) - Quick jump (type label character)
- `[` / `]` - Jump to previous/next mark
- `d` - Delete mark (when in marks overlay)

### Performance & Debug
- `Ctrl+O` - Cycle FPS (toggles through configured choices, e.g., 20 → 30 → 60)
- `Ctrl+D` - Toggle debug panel (shows selection events)
- `c` - Copy details to clipboard (shows toast notification with pane-specific message)
- `q` or `Ctrl+C` - Quit

## Filter System

NEARx provides a powerful query grammar for real-time transaction filtering:

### Filter Syntax

```
acct:alice.near       # Match signer OR receiver
signer:bob.near       # Match signer only
receiver:contract     # Match receiver only
action:FunctionCall   # Match action type
method:ft_transfer    # Match method name
raw:some_text         # Search in raw JSON
freetext              # Match anywhere
```

### Filter Logic
- All filters use AND logic between different field types
- Within each field type, OR logic applies
- Example: `signer:alice.near receiver:token.near` matches transactions where Alice is the signer AND token.near is the receiver

### Common Filter Examples
- `acct:myaccount.near` - Show all transactions involving your account
- `method:ft_transfer` - Show only fungible token transfers
- `action:FunctionCall method:swap` - Show swap function calls
- `raw:error` - Find transactions with errors in their JSON

## Text Selection & Copying

### Terminal Version (Native)
NEARx enables mouse capture for pane navigation. To select text from the terminal:

- **macOS iTerm2**: Hold `Option/Alt` while clicking and dragging
- **macOS Terminal.app**: Hold `Fn` while selecting
- **Linux**: Hold `Shift` while clicking and dragging (GNOME Terminal, Alacritty, xterm, etc.)
- **Windows**: Hold `Shift` while selecting (Windows Terminal, ConEmu)

**Tips**:
- Double-click with modifier key to select entire words (useful for transaction hashes, account names)
- Triple-click with modifier to select entire lines
- **The Details pane has no left/right borders and no indentation** - JSON starts at column 0, making it trivial to select entire lines without fighting borders or padding

### Web Version
Text selection works natively in the browser. Simply click and drag to select - no modifier keys needed.

### Copy Shortcuts
Press `c` to copy pane-specific content to clipboard:
- **Blocks pane**: All transactions in selected block (structured format with metadata)
- **Transactions pane**: Human-readable view + raw JSON payload
- **Details pane**: Full JSON content (what you see in the pane)
- **Visual feedback**: Focused pane border briefly flashes green (Web/Tauri)

## Keyboard Shortcuts Overlay

The keyboard shortcuts help modal is managed through the centralized App state system for consistency across all targets.

### How to Access
- Press `?` to toggle the shortcuts overlay
- Press `Esc` to close the overlay

### Implementation Details
- State tracked in `App.shortcuts_visible: bool`
- Available in Web and Tauri targets
- TUI infrastructure ready for future implementation

## Accessibility & Visual Polish

NEARx implements a comprehensive **4-point focus system** for maximum clarity and accessibility:

### Four-Point Focus Indicator

When a pane receives focus, four visual changes occur simultaneously:

1. **Border Color**: Gray → Bright yellow `#ffcc00` (high contrast)
2. **Border Style**: 1px → 2px bold + inner glow
3. **Background Color**: Dark `#0f131a` → Lighter `#121722` (subtle "lift" effect)
4. **Border Shadow**: Outer glow + inner highlight for extra prominence

**Why Four Changes?**
- **Redundancy**: If one indicator fails (e.g., color blindness), 3 others remain
- **Layered Feedback**: Brain processes multiple cues faster than a single change
- **WCAG AA Compliant**: All color combinations meet accessibility standards

### WCAG AA Compliance

All color ratios verified for accessibility:
- Text on panel: **12.4:1** contrast (AAA!)
- Focused border: **9.8-11.5:1** contrast (AAA!)
- Unfocused border: **3.08:1** contrast (AA minimum)
- Selection highlight: **≥4.5:1** contrast on all backgrounds

**Automated Testing**: See `src/theme.rs:178-232` for WCAG compliance tests that run in CI.

### Screen Reader Support (Web/Tauri)

All interactive elements include ARIA attributes:
- **Panes**: `role="region"` with `aria-label="Blocks panel"` etc.
- **Lists**: `role="listbox"` with `role="option"` for each row
- **Selection**: `aria-selected="true/false"` on focused items
- **Live regions**: `aria-live="polite"` for toast notifications and details updates
- **Keyboard hints**: Screen-reader-only text explains filter usage

**Keyboard-only operation**: Entire app navigable without mouse. Tab focuses panes, arrows navigate lists, Space/Enter activate items.

### Visual Enhancements (Web/Tauri)

- **Smooth transitions**: All focus changes animated with 150ms ease
- **Dimmed selections**: Unfocused pane selections shown at 50% opacity (text stays readable)
- **Hover feedback**: Rows show pointer cursor + subtle highlight on hover
- **Loading states**: Archival block fetches show pulsing "⏳ Loading…" indicator
- **Copy animation**: Border flashes green for 300ms after successful copy
- **Help modal**: Press `?` for comprehensive keyboard shortcuts reference

### Browser Compatibility

**Tested browsers**:
- ✅ Chrome/Edge (latest)
- ✅ Firefox (latest)
- ✅ Safari (latest)

**Zoom support**: UI tested at 100%, 125%, 150%, 200% zoom levels
**High contrast mode**: Forced colors media query support for accessibility

## Advanced Features

### Owned Accounts Filter
- Press `Ctrl+U` to toggle showing only transactions involving your accounts
- Accounts automatically discovered from `~/.near-credentials`
- Visual indicator in footer when active

### History Search (Native Only)
- Press `Ctrl+F` to open search overlay
- Search through all previously seen transactions
- Uses same filter syntax as real-time filtering
- Results stored in local SQLite database

### Jump Marks
- Bookmark interesting transactions for quick navigation
- Marks persist across sessions when pinned
- Quick jump with `'` followed by mark label

For configuration options, see [Chapter 3: Configuration](03-configuration.md).
For architecture details, see [Chapter 4: Architecture](04-architecture.md).