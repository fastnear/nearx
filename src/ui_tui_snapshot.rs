use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs, Wrap,
    },
    Frame,
};

use crate::json_syntax::colorize_json;
use crate::theme::Theme;
use crate::ui_snapshot::{UiBlockRow, UiSnapshot, UiTxRow};

/// Draw the main NEARx UI from a [`UiSnapshot`] and [`Theme`].
///
/// This is the TUI reference layout that web/Tauri should mirror:
/// - Header tabs
/// - Filter bar
/// - Blocks / Txs / Details panes
/// - Footer status line
pub fn draw_from_snapshot(f: &mut Frame, area: Rect, snapshot: &UiSnapshot, _theme: &Theme) {
    // Show warning if terminal is too small to be usable
    const MIN_WIDTH: u16 = 60;
    const MIN_HEIGHT: u16 = 15;

    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        let warning_text = format!(
            "Terminal too small!\n\nMinimum size: {}×{}\nCurrent size: {}×{}\n\nPlease resize your terminal.",
            MIN_WIDTH, MIN_HEIGHT, area.width, area.height
        );

        let warning = Paragraph::new(warning_text)
            .alignment(ratatui::layout::Alignment::Center)
            .style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(" NEARx "),
            );

        f.render_widget(Clear, area);
        f.render_widget(warning, area);
        return;
    }

    // Simple chrome for now: header + filter + body + footer.
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // header
            Constraint::Length(1), // filter
            Constraint::Min(0),    // body
            Constraint::Length(1), // footer
        ])
        .split(area);

    header_from_snapshot(f, chunks[0], snapshot);
    filter_from_snapshot(f, chunks[1], snapshot);
    body_from_snapshot(f, chunks[2], snapshot, _theme);
    footer_from_snapshot(f, chunks[3], snapshot);
}

fn header_from_snapshot(f: &mut Frame, area: Rect, snapshot: &UiSnapshot) {
    let titles = ["Blocks", "Tx Hashes", "Tx Details"]
        .into_iter()
        .map(|t| Line::from(Span::raw(t)))
        .collect::<Vec<_>>();

    let tabs = Tabs::new(titles)
        .select(snapshot.pane)
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_type(BorderType::Plain),
        )
        .divider(Span::raw(" | "));

    f.render_widget(tabs, area);
}

fn filter_from_snapshot(f: &mut Frame, area: Rect, snapshot: &UiSnapshot) {
    let filter_text = snapshot.filter_query.as_str();
    let focused = snapshot.filter_focused;

    // Collapsed rule when idle and no filter text.
    if area.height <= 1 && !focused && filter_text.is_empty() {
        let rule = Block::default()
            .borders(Borders::BOTTOM)
            .border_type(BorderType::Plain)
            .border_style(Style::default().fg(Color::DarkGray));
        f.render_widget(rule, area);
        return;
    }

    let border_color = if focused {
        Color::Cyan
    } else if !filter_text.is_empty() {
        Color::LightCyan
    } else {
        Color::DarkGray
    };

    let text_color = if filter_text.is_empty() && !focused {
        Color::DarkGray
    } else {
        Color::White
    };

    let hint = "(Press / or f to filter • comma=OR, space=AND • e.g. signer:alice,bob)";
    let display_text = if filter_text.is_empty() && !focused {
        hint
    } else {
        filter_text
    };

    let paragraph = Paragraph::new(display_text)
        .style(Style::default().fg(text_color))
        .block(
            Block::default()
                .title(" Filter ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(border_color)),
        );

    f.render_widget(paragraph, area);
}

fn body_from_snapshot(f: &mut Frame, area: Rect, snapshot: &UiSnapshot, theme: &Theme) {
    // Responsive layout: on narrow terminals stack panes vertically.
    const NARROW_THRESHOLD: u16 = 80;
    let is_narrow = area.width < NARROW_THRESHOLD;

    if is_narrow {
        // Narrow: Blocks 20% → Txs 20% → Details 60%.
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(20),
                Constraint::Percentage(20),
                Constraint::Percentage(60),
            ])
            .split(area);

        render_blocks_pane(f, rows[0], snapshot);
        render_txs_pane(f, rows[1], snapshot);
        render_details_pane(f, rows[2], snapshot, theme);
    } else {
        // Wide: top (blocks+tx) vs bottom (details).
        let top_ratio: u16 = 60;
        let bot_ratio = 100u16.saturating_sub(top_ratio);

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(top_ratio),
                Constraint::Percentage(bot_ratio),
            ])
            .split(area);

        let top_cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Ratio(1, 3), Constraint::Ratio(2, 3)])
            .split(rows[0]);

        render_blocks_pane(f, top_cols[0], snapshot);
        render_txs_pane(f, top_cols[1], snapshot);
        render_details_pane(f, rows[1], snapshot, theme);
    }
}

fn render_blocks_pane(f: &mut Frame, area: Rect, snapshot: &UiSnapshot) {
    let blocks_focused = snapshot.pane == 0;

    f.render_widget(Clear, area);

    let mut state = ListState::default();
    if let Some(sel_idx) = snapshot
        .blocks
        .iter()
        .position(|b: &UiBlockRow| b.is_selected)
    {
        state.select(Some(sel_idx));
    }

    let items: Vec<ListItem> = snapshot
        .blocks
        .iter()
        .map(|b| {
            let text = format!("#{}  | {} txs | {}", b.height, b.tx_count, b.when);
            let item = ListItem::new(text);
            if b.available {
                item
            } else {
                item.style(Style::default().fg(Color::DarkGray))
            }
        })
        .collect();

    let title = if snapshot.viewing_cached {
        " Blocks (cached) · ← Recent ".to_string()
    } else if !items.is_empty() && snapshot.blocks.len() < snapshot.blocks_total {
        format!(
            " Blocks ({} / {}) ",
            snapshot.blocks.len(),
            snapshot.blocks_total
        )
    } else {
        " Blocks ".to_string()
    };

    let border_color = if blocks_focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let widget = List::new(items)
        .highlight_style(
            Style::default()
                .fg(Color::White)
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("• ")
        .block(
            Block::default()
                .title(if blocks_focused {
                    title.yellow().bold()
                } else {
                    title.into()
                })
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(border_color)),
        );

    f.render_stateful_widget(widget, area, &mut state);
}

fn render_txs_pane(f: &mut Frame, area: Rect, snapshot: &UiSnapshot) {
    let txs_focused = snapshot.pane == 1;

    f.render_widget(Clear, area);

    let mut state = ListState::default();
    if !snapshot.txs.is_empty() {
        if let Some(sel_idx) = snapshot.txs.iter().position(|t: &UiTxRow| t.is_selected) {
            state.select(Some(sel_idx));
        }
    }

    let items: Vec<ListItem> = snapshot
        .txs
        .iter()
        .map(|t| {
            let label = if !t.signer_id.is_empty() || !t.receiver_id.is_empty() {
                if !t.signer_id.is_empty() && !t.receiver_id.is_empty() {
                    format!("{} → {}", t.signer_id, t.receiver_id)
                } else {
                    t.signer_id.clone()
                }
            } else {
                t.hash.clone()
            };
            ListItem::new(label)
        })
        .collect();

    let title = if snapshot.txs_total == 0 {
        " Transactions ".to_string()
    } else {
        let selected = snapshot
            .txs
            .iter()
            .position(|t| t.is_selected)
            .map(|i| i + 1)
            .unwrap_or(0);
        format!(
            " Transactions ({} / {}) ",
            selected.max(1).min(snapshot.txs_total),
            snapshot.txs_total
        )
    };

    let border_color = if txs_focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let widget = List::new(items)
        .highlight_style(
            Style::default()
                .fg(Color::White)
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("• ")
        .block(
            Block::default()
                .title(if txs_focused {
                    title.yellow().bold()
                } else {
                    title.into()
                })
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(border_color)),
        );

    f.render_stateful_widget(widget, area, &mut state);
}

fn render_details_pane(f: &mut Frame, area: Rect, snapshot: &UiSnapshot, theme: &Theme) {
    let details_focused = snapshot.pane == 2;

    f.render_widget(Clear, area);

    let border_color = if details_focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let title = if details_focused {
        if snapshot.details_fullscreen {
            " Transaction Details - press 'c' to copy • spacebar exits fullscreen "
        } else {
            " Transaction Details - press 'c' to copy • spacebar to expand "
        }
    } else {
        " Transaction Details "
    };

    let is_loading = snapshot.loading_block.is_some();

    let paragraph = if !is_loading {
        // JSON syntax highlighting via colorize_json
        let lines = colorize_json(&snapshot.details, theme);
        Paragraph::new(lines).wrap(Wrap { trim: false }).block(
            Block::default()
                .title(if details_focused {
                    title.yellow().bold()
                } else {
                    title.into()
                })
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(border_color)),
        )
    } else {
        // Loading state: plain text
        Paragraph::new(snapshot.details.as_str())
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(Color::White))
            .block(
                Block::default()
                    .title(if details_focused {
                        title.yellow().bold()
                    } else {
                        title.into()
                    })
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(border_color)),
            )
    };

    f.render_widget(paragraph, area);
}

fn footer_from_snapshot(f: &mut Frame, area: Rect, snapshot: &UiSnapshot) {
    let mut parts: Vec<String> = Vec::new();

    parts.push(format!("Blocks: {}", snapshot.blocks_total));
    parts.push(format!("Txs: {}", snapshot.txs_total));

    if let Some(h) = snapshot.selected_block_height {
        parts.push(format!("Block #{h}"));
    }

    if let Some(lb) = snapshot.loading_block {
        parts.push(format!("⏳ archival #{lb}"));
    }

    let footer_text = parts.join("  •  ");

    let line = Line::from(Span::styled(
        footer_text,
        Style::default().fg(Color::DarkGray),
    ));

    let paragraph = Paragraph::new(line).block(
        Block::default()
            .borders(Borders::TOP)
            .border_type(BorderType::Plain),
    );

    f.render_widget(paragraph, area);
}
