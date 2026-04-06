use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap,
    },
};

use crate::{
    app::App,
    model::{LogLevel, Step, StepState},
};

const ACCENT: Color = Color::Rgb(255, 191, 0);
const BRAND: Color = Color::Rgb(0, 190, 180);
const DIM: Color = Color::DarkGray;
const MUTED: Color = Color::Rgb(100, 100, 100);

// Returns the max scroll offset for the current content
pub fn render(app: &App, frame: &mut Frame) -> usize {
    let area = frame.area();

    let inner = if app.fullscreen {
        area
    } else {
        let content_width = 76u16.min(area.width.saturating_sub(2));
        let content_height = (area.height * 4 / 5).max(10);
        let h_pad = area.width.saturating_sub(content_width) / 2;
        let v_pad = area.height.saturating_sub(content_height) / 2;
        let container = Rect::new(
            area.x + h_pad,
            area.y + v_pad,
            content_width,
            content_height,
        );

        let border = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(MUTED));
        let result = border.inner(container);
        frame.render_widget(border, container);
        result
    };

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let max_scroll = render_flow(app, frame, layout[0]);
    render_status_bar(app, frame, layout[1]);

    if app.show_help {
        render_help_overlay(app, frame, inner);
    }

    max_scroll
}

fn render_flow(app: &App, frame: &mut Frame, area: Rect) -> usize {
    let padded = Rect::new(
        area.x + 2,
        area.y,
        area.width.saturating_sub(4),
        area.height,
    );

    // Build all lines for the scrollable flow
    let mut lines: Vec<Line> = Vec::new();

    // Header: logo + host info
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled("  ░▒▓ ", Style::default().fg(BRAND)),
        Span::styled(
            "orda",
            Style::default().fg(BRAND).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            ".chat",
            Style::default().fg(DIM).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" installer", Style::default().fg(MUTED)),
    ]));
    lines.push(Line::styled(
        "      byoc communication solution",
        Style::default().fg(MUTED),
    ));
    let separator = "─".repeat(padded.width as usize);
    lines.push(Line::styled(separator, Style::default().fg(MUTED)));
    lines.push(Line::raw(""));

    let h = &app.host;
    let version = env!("CARGO_PKG_VERSION");

    if h.arch.is_empty() {
        lines.push(Line::styled("  Detecting...", Style::default().fg(MUTED)));
    } else {
        let label = |s: &str| -> Span<'static> {
            Span::styled(format!("  {:<12}", s), Style::default().fg(DIM))
        };
        let val =
            |s: String| -> Span<'static> { Span::styled(s, Style::default().fg(Color::White)) };
        let check = |ok: bool, name: &str| -> Line<'static> {
            let icon = if ok {
                Span::styled("■ ", Style::default().fg(Color::Green))
            } else {
                Span::styled("✗ ", Style::default().fg(Color::Red))
            };
            Line::from(vec![
                Span::styled(format!("  {:<12}", ""), Style::default()),
                icon,
                Span::styled(
                    name.to_string(),
                    Style::default().fg(if ok { Color::White } else { Color::Red }),
                ),
            ])
        };

        lines.push(Line::from(vec![label("hostname"), val(h.hostname.clone())]));
        lines.push(Line::from(vec![
            label("platform"),
            val(format!("{}/{}", h.os, h.arch)),
        ]));
        lines.push(Line::from(vec![
            label("public ip"),
            val(h.public_ip.clone()),
        ]));
        lines.push(Line::from(vec![
            label("installer"),
            val(format!("v{version}")),
        ]));
        lines.push(Line::from(vec![
            label("issues"),
            Span::styled("github.com/orda-oss/installer", Style::default().fg(MUTED)),
        ]));
        lines.push(Line::raw(""));
        lines.push(check(h.connectivity, "network reachable"));
        lines.push(check(h.docker, "docker installed"));
    }
    lines.push(Line::raw(""));

    // Divider
    let div = "▬".repeat(padded.width as usize);
    lines.push(Line::styled(div, Style::default().fg(MUTED)));
    lines.push(Line::raw(""));

    // Unsupported OS block
    if app.unsupported_os {
        lines.push(Line::styled(
            "✗ UNSUPPORTED PLATFORM",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            "  Orda server requires Linux (amd64 or arm64).",
            Style::default().fg(Color::White),
        ));
        lines.push(Line::styled(
            "  Run this installer on a Linux VM or dedicated server.",
            Style::default().fg(MUTED),
        ));
        lines.push(Line::raw(""));
        lines.push(Line::styled("  Q TO EXIT", Style::default().fg(DIM)));

        let total_lines = lines.len();
        let visible_height = padded.height as usize;
        let scroll_offset = total_lines.saturating_sub(visible_height);
        let widget = Paragraph::new(lines)
            .scroll((scroll_offset as u16, 0))
            .wrap(Wrap { trim: false });
        frame.render_widget(widget, padded);
        return 0;
    }

    // Step flow
    for &step in Step::FLOW {
        let state = app.step_state(step);

        // Don't show Complete unless it actually succeeded
        if step == Step::Complete && *state != StepState::Success {
            continue;
        }

        // Pending steps shown as dim placeholders
        if *state == StepState::Pending {
            lines.push(step_header(step, state, ' '));
            continue;
        }

        // Step header with icon
        let spinner = app.spinner_char();
        lines.push(step_header(step, state, spinner));

        // Step-specific content
        match step {
            Step::License => {
                render_license_section(app, &mut lines);
            }
            Step::Security if app.security_input_active() => {
                render_security_section(app, &mut lines);
            }
            Step::Complete if *state == StepState::Success => {
                render_complete_section(app, &mut lines, padded.height as usize);
            }
            _ => {
                // Show recent logs for this step (max N lines)
                let logs = app.step_logs(step);
                let max = if app.verbose { usize::MAX } else { 5 };
                let start = logs.len().saturating_sub(max);
                for entry in &logs[start..] {
                    if entry.text.is_empty() {
                        continue;
                    }
                    let style = log_style(entry.level);
                    lines.push(Line::styled(format!("  {}", entry.text), style));
                }
            }
        }

        lines.push(Line::raw(""));
    }

    let total_lines = lines.len();
    let visible_height = padded.height as usize;
    let max_scroll = total_lines.saturating_sub(visible_height);

    // None = auto-scroll to bottom, Some(n) = user-controlled
    let scroll_offset = match app.scroll_offset {
        None => max_scroll,
        Some(s) => s.min(max_scroll),
    };

    let widget = Paragraph::new(lines)
        .scroll((scroll_offset as u16, 0))
        .wrap(Wrap { trim: false });
    frame.render_widget(widget, padded);

    if total_lines > visible_height {
        let scrollbar_area = Rect::new(padded.x + padded.width, padded.y, 1, padded.height);
        let mut scrollbar_state = ScrollbarState::new(total_lines).position(scroll_offset);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight).style(Style::default().fg(MUTED)),
            scrollbar_area,
            &mut scrollbar_state,
        );
    }

    max_scroll
}

fn step_header(step: Step, state: &StepState, spinner: char) -> Line<'static> {
    let (icon, icon_style) = step_icon(state, spinner);
    let label_style = match state {
        StepState::Running => Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
        StepState::Success => Style::default().fg(Color::White),
        StepState::Failed(_) => Style::default().fg(Color::Red),
        _ => Style::default().fg(DIM),
    };

    Line::from(vec![
        Span::styled(format!("{icon} "), icon_style),
        Span::styled(step.label(), label_style),
    ])
}

fn step_icon(state: &StepState, spinner: char) -> (String, Style) {
    match state {
        StepState::Success => ("■".to_string(), Style::default().fg(Color::Green)),
        StepState::Running => (spinner.to_string(), Style::default().fg(ACCENT)),
        StepState::Pending => ("·".to_string(), Style::default().fg(DIM)),
        StepState::Failed(_) => ("✗".to_string(), Style::default().fg(Color::Red)),
        StepState::Skipped(_) => ("–".to_string(), Style::default().fg(DIM)),
    }
}

fn render_license_section(app: &App, lines: &mut Vec<Line<'static>>) {
    if app.license_input_active() {
        // Editable input
        let cursor = if app.spinner_tick % 4 < 2 { "▌" } else { " " };
        lines.push(Line::from(vec![
            Span::styled("  ▸ ", Style::default().fg(ACCENT)),
            Span::styled(app.input_buffer.clone(), Style::default().fg(Color::White)),
            Span::styled(cursor.to_string(), Style::default().fg(MUTED)),
        ]));

        // Hint
        let logs = app.step_logs(Step::License);
        let last_error = logs.iter().rev().find(|e| e.level == LogLevel::Error);
        if let Some(err) = last_error {
            lines.push(Line::styled(
                format!("  {}", err.text),
                Style::default().fg(Color::Red),
            ));
        } else if app.input_buffer.is_empty() {
            lines.push(Line::styled(
                "  Paste or type your license key",
                Style::default().fg(MUTED),
            ));
        } else {
            lines.push(Line::styled(
                "  ENTER to continue",
                Style::default().fg(DIM),
            ));
        }
    } else {
        // Completed: show masked key
        let key = &app.context.license_key;
        let display = if key.len() > 8 {
            format!("{}····{}", &key[..4], &key[key.len() - 4..])
        } else {
            key.clone()
        };
        lines.push(Line::styled(
            format!("  {display}"),
            Style::default().fg(DIM),
        ));
    }
}

fn render_security_section(app: &App, lines: &mut Vec<Line<'static>>) {
    let sel = app.security_selection;
    let secs = app.security_countdown();

    let opt = |idx: usize, label: &str| -> Line<'static> {
        if sel == idx {
            let mut spans = vec![
                Span::styled("  ▸ ", Style::default().fg(ACCENT)),
                Span::styled(
                    label.to_string(),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
            ];
            // Show countdown on the selected option
            spans.push(Span::styled(
                format!("  ({secs}s)"),
                Style::default().fg(MUTED),
            ));
            Line::from(spans)
        } else {
            Line::styled(format!("    {label}"), Style::default().fg(DIM))
        }
    };

    lines.push(opt(0, "INSTALL FIREWALL + FAIL2BAN"));
    lines.push(opt(1, "SKIP"));
}

fn render_complete_section(app: &App, lines: &mut Vec<Line<'static>>, visible_height: usize) {
    let dir = app.context.orda_dir.to_string_lossy();

    let section_start = lines.len();

    let label = |s: &str| -> Span<'static> {
        Span::styled(format!("  {:<14}", s), Style::default().fg(DIM))
    };
    let val = |s: String| -> Span<'static> { Span::styled(s, Style::default().fg(Color::White)) };

    lines.push(Line::raw(""));
    lines.push(Line::styled(
        "  ■ ■ ■  SERVER INSTALLED  ■ ■ ■",
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
    ));
    lines.push(Line::raw(""));

    // Services
    let svc = |name: &str, desc: &str| -> Line<'static> {
        Line::from(vec![
            Span::styled(format!("  {:<14}", ""), Style::default()),
            Span::styled(format!("{:<18}", name), Style::default().fg(ACCENT)),
            Span::styled(desc.to_string(), Style::default().fg(MUTED)),
        ])
    };
    lines.push(Line::from(vec![
        label("services"),
        Span::styled(format!("{:<18}", "alacahoyuk"), Style::default().fg(ACCENT)),
        Span::styled("Server engine", Style::default().fg(MUTED)),
    ]));
    lines.push(svc("caddy", "Reverse proxy + TLS"));
    lines.push(svc("livekit", "Voice & video"));
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![label("directory"), val(dir.to_string())]));
    lines.push(Line::from(vec![
        label("data"),
        Span::styled(format!("{dir}/data"), Style::default().fg(MUTED)),
    ]));
    lines.push(Line::from(vec![
        label("tls"),
        Span::styled(format!("{dir}/tls"), Style::default().fg(MUTED)),
    ]));
    lines.push(Line::from(vec![
        label("reference"),
        Span::styled(format!("{dir}/README.txt"), Style::default().fg(MUTED)),
    ]));
    lines.push(Line::raw(""));

    // Next step
    let divider = "  ".to_string() + &"─".repeat(42);
    lines.push(Line::styled(divider, Style::default().fg(MUTED)));
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        "  Your server should be online now.",
        Style::default().fg(Color::White),
    ));
    lines.push(Line::styled(
        "  Go back to the desktop app and click continue",
        Style::default().fg(MUTED),
    ));
    lines.push(Line::styled(
        "  to log in and create invites for others to join.",
        Style::default().fg(MUTED),
    ));

    // Bottom padding to visually center the complete section
    let section_lines = lines.len() - section_start;
    let pad = visible_height.saturating_sub(section_lines) / 2;
    for _ in 0..pad {
        lines.push(Line::raw(""));
    }
}

fn log_style(level: LogLevel) -> Style {
    match level {
        LogLevel::Info => Style::default().fg(Color::White),
        LogLevel::Success => Style::default().fg(Color::Green),
        LogLevel::Command => Style::default().fg(MUTED),
        LogLevel::Error => Style::default().fg(Color::Red),
        LogLevel::Dim => Style::default().fg(DIM),
    }
}

fn render_status_bar(app: &App, frame: &mut Frame, area: Rect) {
    let secs = app.elapsed.as_secs();
    let elapsed = format!("{:01}:{:02}", secs / 60, secs % 60);

    let left = if app.done || app.should_quit {
        Span::styled(" q exit", Style::default().fg(MUTED))
    } else if app.abort_requested {
        Span::styled(
            " CLEANING UP...",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
    } else if app.unsupported_os {
        Span::styled(" q exit", Style::default().fg(MUTED))
    } else {
        Span::styled(" ctrl+h help", Style::default().fg(MUTED))
    };

    let right = Span::styled(format!("{elapsed} "), Style::default().fg(MUTED));
    let left_len = left.width();
    let right_len = right.width();
    let fill = (area.width as usize).saturating_sub(left_len + right_len);
    let line = Line::from(vec![left, Span::raw(" ".repeat(fill)), right]);
    frame.render_widget(Paragraph::new(line), area);
}

fn render_help_overlay(_app: &App, frame: &mut Frame, area: Rect) {
    let k = Style::default()
        .fg(Color::White)
        .add_modifier(Modifier::BOLD);
    let d = Style::default().fg(MUTED);
    let bg = Style::default().bg(Color::Black);

    let row = |key: &str, desc: &str| -> Line<'static> {
        Line::from(vec![
            Span::styled(format!("  {:<16}", key), k.bg(Color::Black)),
            Span::styled(desc.to_string(), d.bg(Color::Black)),
        ])
    };

    let lines = vec![
        Line::styled("", bg),
        Line::styled(
            "  KEYBOARD SHORTCUTS",
            Style::default()
                .fg(Color::White)
                .bg(Color::Black)
                .add_modifier(Modifier::BOLD),
        ),
        Line::styled("", bg),
        row("enter", "Submit / confirm"),
        row("ctrl+backspace", "Clear input"),
        row("esc", "Clear input"),
        Line::styled("", bg),
        row("up / down", "Scroll or select"),
        row("shift+up / down", "Scroll fast"),
        row("g", "Scroll to top"),
        row("G", "Follow output"),
        row("ctrl+f", "Toggle fullscreen"),
        Line::styled("", bg),
        row("ctrl+c", "Abort installation"),
        row("ctrl+c ctrl+c", "Force quit"),
        row("q", "Exit (when done)"),
        Line::styled("", bg),
        Line::styled(
            "  Press any key to close (ctrl+h to toggle)",
            Style::default().fg(DIM).bg(Color::Black),
        ),
        Line::styled("", bg),
    ];

    let height = (lines.len() as u16 + 2).min(area.height);
    let width = 46u16.min(area.width.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let overlay = Rect::new(x, y, width, height);

    // Clear all cells in the overlay area, then fill with black
    frame.render_widget(Clear, overlay);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .style(bg);
    let inner = block.inner(overlay);
    frame.render_widget(block, overlay);
    frame.render_widget(Paragraph::new(lines), inner);
}
