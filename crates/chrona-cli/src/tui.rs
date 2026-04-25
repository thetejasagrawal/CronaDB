//! Interactive terminal UI for browsing a CronaDB file.
//!
//! Built on `ratatui` + `crossterm`. The defining feature is the **time
//! cursor** in the header bar: scrolling it backwards or forwards instantly
//! re-renders the selected node's neighborhood as it looked at that moment,
//! so the temporal model is visible the way a slider in a video editor is.
//!
//! Run with `chrona tui <path>`.

use std::io::{self, Stdout};
use std::path::PathBuf;
use std::time::Duration;

use chrona_core::{Db, EdgeView, Node, Snapshot, Stats, Ts};
use chrona_query::{execute, parse, render};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap,
};
use ratatui::{Frame, Terminal};

use crate::anyhow_like::{BoxError, Result};

const NS_PER_DAY: i64 = 86_400 * 1_000_000_000;

/// Run the TUI against the database at `path`. Returns when the user quits.
pub fn run(path: PathBuf) -> Result<()> {
    if !std::io::IsTerminal::is_terminal(&io::stdout()) {
        return Err("chrona tui needs an interactive terminal (stdout is not a TTY)".into());
    }

    let db = Db::open(&path)?;
    let mut terminal = setup_terminal()?;

    // Make sure we always restore the terminal even on panic.
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore_terminal();
        original_hook(info);
    }));

    let result = (|| -> Result<()> {
        let mut app = App::new(path)?;
        app.refresh(&db)?;
        loop {
            terminal.draw(|f| app.render(f))?;
            if event::poll(Duration::from_millis(150))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    match app.handle_key(&db, key.code, key.modifiers)? {
                        Outcome::Continue => {}
                        Outcome::Quit => break,
                    }
                }
            }
        }
        Ok(())
    })();

    let _ = restore_terminal();
    result
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

fn restore_terminal() -> Result<()> {
    let mut stdout = io::stdout();
    execute!(stdout, LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Normal,
    Query,
}

enum Outcome {
    Continue,
    Quit,
}

struct App {
    path: PathBuf,
    mode: Mode,
    show_help: bool,
    nodes: Vec<NodeRow>,
    node_state: ListState,
    edges: Vec<EdgeView>,
    edge_state: ListState,
    stats: Stats,
    /// Time at which we render the right-hand neighborhood. Defaults to
    /// `Ts::now()`; `+/-` and friends move it. `None` means "live = now"
    /// (re-evaluated on every refresh).
    time_cursor: Option<Ts>,
    query_input: String,
    query_status: String,
    query_lines: Vec<String>,
}

struct NodeRow {
    ext_id: String,
    type_name: Option<String>,
}

impl App {
    fn new(path: PathBuf) -> Result<Self> {
        Ok(Self {
            path,
            mode: Mode::Normal,
            show_help: false,
            nodes: Vec::new(),
            node_state: ListState::default(),
            edges: Vec::new(),
            edge_state: ListState::default(),
            stats: Stats::default(),
            time_cursor: None,
            query_input: String::new(),
            query_status: String::new(),
            query_lines: Vec::new(),
        })
    }

    /// Reload nodes and stats from the database. Resets selection if needed.
    fn refresh(&mut self, db: &Db) -> Result<()> {
        let snap = db.begin_read()?;
        self.stats = snap.stats()?;
        let raw_nodes = snap.all_nodes()?;
        self.nodes = raw_nodes
            .into_iter()
            .map(|n: Node| {
                let type_name = match n.type_id {
                    Some(id) => snap.resolve_string(id).ok(),
                    None => None,
                };
                NodeRow {
                    ext_id: n.ext_id,
                    type_name,
                }
            })
            .collect();
        if self.nodes.is_empty() {
            self.node_state.select(None);
        } else if self.node_state.selected().is_none() {
            self.node_state.select(Some(0));
        } else if self.node_state.selected().unwrap() >= self.nodes.len() {
            self.node_state.select(Some(self.nodes.len() - 1));
        }
        self.refresh_edges(&snap)?;
        Ok(())
    }

    fn refresh_edges(&mut self, snap: &Snapshot) -> Result<()> {
        self.edges.clear();
        let Some(idx) = self.node_state.selected() else {
            self.edge_state.select(None);
            return Ok(());
        };
        let Some(row) = self.nodes.get(idx) else {
            self.edge_state.select(None);
            return Ok(());
        };
        let Some(node_id) = snap.get_node_id(&row.ext_id)? else {
            self.edge_state.select(None);
            return Ok(());
        };
        let when = self.effective_time();
        let raw = snap.neighbors_as_of(node_id, when)?;
        for e in raw {
            let view = snap.view_edge(&e)?;
            self.edges.push(view);
        }
        if self.edges.is_empty() {
            self.edge_state.select(None);
        } else {
            self.edge_state.select(Some(0));
        }
        Ok(())
    }

    fn effective_time(&self) -> Ts {
        self.time_cursor.unwrap_or_else(Ts::now)
    }

    fn handle_key(&mut self, db: &Db, code: KeyCode, mods: KeyModifiers) -> Result<Outcome> {
        if self.show_help {
            // Anything dismisses help.
            self.show_help = false;
            return Ok(Outcome::Continue);
        }
        match self.mode {
            Mode::Normal => self.handle_key_normal(db, code, mods),
            Mode::Query => self.handle_key_query(db, code, mods),
        }
    }

    fn handle_key_normal(&mut self, db: &Db, code: KeyCode, mods: KeyModifiers) -> Result<Outcome> {
        match code {
            KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => return Ok(Outcome::Quit),
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Char('r') => self.refresh(db)?,
            KeyCode::Char(':') | KeyCode::Char('/') | KeyCode::Char('i') => {
                self.mode = Mode::Query;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.move_selection(1, db)?;
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.move_selection(-1, db)?;
            }
            KeyCode::Char('g') if !self.nodes.is_empty() => {
                self.node_state.select(Some(0));
                let snap = db.begin_read()?;
                self.refresh_edges(&snap)?;
            }
            KeyCode::Char('G') if !self.nodes.is_empty() => {
                self.node_state.select(Some(self.nodes.len() - 1));
                let snap = db.begin_read()?;
                self.refresh_edges(&snap)?;
            }
            KeyCode::Char('n') => {
                // Reset time to "live = now".
                self.time_cursor = None;
                let snap = db.begin_read()?;
                self.refresh_edges(&snap)?;
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                self.bump_time(NS_PER_DAY, db)?;
            }
            KeyCode::Char('-') | KeyCode::Char('_') => {
                self.bump_time(-NS_PER_DAY, db)?;
            }
            KeyCode::Char(']') => {
                let step = if mods.contains(KeyModifiers::SHIFT) {
                    NS_PER_DAY * 30
                } else {
                    NS_PER_DAY * 7
                };
                self.bump_time(step, db)?;
            }
            KeyCode::Char('[') => {
                let step = if mods.contains(KeyModifiers::SHIFT) {
                    NS_PER_DAY * 30
                } else {
                    NS_PER_DAY * 7
                };
                self.bump_time(-step, db)?;
            }
            _ => {}
        }
        Ok(Outcome::Continue)
    }

    fn handle_key_query(&mut self, db: &Db, code: KeyCode, _mods: KeyModifiers) -> Result<Outcome> {
        match code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Enter => self.run_query(db),
            KeyCode::Backspace => {
                self.query_input.pop();
            }
            KeyCode::Char(c) => self.query_input.push(c),
            _ => {}
        }
        Ok(Outcome::Continue)
    }

    fn run_query(&mut self, db: &Db) {
        let q = self.query_input.trim();
        if q.is_empty() {
            self.query_status = "(empty query)".into();
            self.query_lines.clear();
            return;
        }
        match (|| -> std::result::Result<String, BoxError> {
            let snap = db.begin_read()?;
            let ast = parse(q)?;
            let result = execute(&snap, ast)?;
            Ok(render(&result))
        })() {
            Ok(out) => {
                let lines: Vec<String> = out.lines().map(|l| l.to_string()).collect();
                self.query_status = format!("{} line(s)", lines.len());
                self.query_lines = lines;
                self.mode = Mode::Normal;
            }
            Err(e) => {
                self.query_status = format!("error: {}", e);
                self.query_lines.clear();
            }
        }
    }

    fn bump_time(&mut self, delta_ns: i64, db: &Db) -> Result<()> {
        let base = self.effective_time().raw();
        let next = base.saturating_add(delta_ns);
        self.time_cursor = Some(Ts::from_nanos(next));
        let snap = db.begin_read()?;
        self.refresh_edges(&snap)?;
        Ok(())
    }

    fn move_selection(&mut self, delta: i32, db: &Db) -> Result<()> {
        if self.nodes.is_empty() {
            return Ok(());
        }
        let len = self.nodes.len() as i32;
        let cur = self.node_state.selected().unwrap_or(0) as i32;
        let next = ((cur + delta).rem_euclid(len)) as usize;
        self.node_state.select(Some(next));
        let snap = db.begin_read()?;
        self.refresh_edges(&snap)?;
        Ok(())
    }

    // ------------- Rendering -------------

    fn render(&mut self, f: &mut Frame<'_>) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // header
                Constraint::Min(8),    // body (nodes + edges)
                Constraint::Length(3), // query input
                Constraint::Length(8), // results
                Constraint::Length(1), // footer help
            ])
            .split(f.area());

        self.render_header(f, chunks[0]);
        self.render_body(f, chunks[1]);
        self.render_query_input(f, chunks[2]);
        self.render_results(f, chunks[3]);
        self.render_footer(f, chunks[4]);

        if self.show_help {
            self.render_help_overlay(f);
        }
    }

    fn render_header(&self, f: &mut Frame<'_>, area: Rect) {
        let when = self.effective_time();
        let when_label = if self.time_cursor.is_none() {
            "now (live)".to_string()
        } else {
            short_ts(when)
        };

        let line = Line::from(vec![
            Span::styled(
                " CronaDB ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                self.path.display().to_string(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled(
                format!(
                    "{} nodes  ·  {} edges  ·  {} events",
                    self.stats.node_count, self.stats.edge_count, self.stats.event_count
                ),
                Style::default().fg(Color::DarkGray),
            ),
            Span::raw("   "),
            Span::styled(
                format!("time: {}", when_label),
                Style::default().fg(Color::Yellow),
            ),
        ]);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::DarkGray));
        let p = Paragraph::new(line).block(block);
        f.render_widget(p, area);
    }

    fn render_body(&mut self, f: &mut Frame<'_>, area: Rect) {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
            .split(area);

        // Nodes
        let items: Vec<ListItem<'_>> = self
            .nodes
            .iter()
            .map(|n| {
                let line = Line::from(vec![
                    Span::raw(format!(" {:<14} ", truncate(&n.ext_id, 14))),
                    Span::styled(
                        n.type_name.as_deref().unwrap_or("-").to_string(),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]);
                ListItem::new(line)
            })
            .collect();

        let nodes_list = List::new(items)
            .block(
                Block::default()
                    .title(Span::styled(
                        format!(" Nodes ({}) ", self.nodes.len()),
                        Style::default().add_modifier(Modifier::BOLD),
                    ))
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::Cyan)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("► ");

        f.render_stateful_widget(nodes_list, cols[0], &mut self.node_state);

        // Edges from the selected node, at the time cursor.
        let selected_label = self
            .node_state
            .selected()
            .and_then(|i| self.nodes.get(i))
            .map(|n| n.ext_id.as_str())
            .unwrap_or("(no selection)");

        let title_label = format!(
            " Edges from {:?} @ {} ({}) ",
            selected_label,
            if self.time_cursor.is_none() {
                "now".to_string()
            } else {
                short_ts(self.effective_time())
            },
            self.edges.len(),
        );

        let edge_items: Vec<ListItem<'_>> = self
            .edges
            .iter()
            .map(|e| ListItem::new(format_edge_line(e)))
            .collect();

        let edges_list = List::new(edge_items)
            .block(
                Block::default()
                    .title(Span::styled(
                        title_label,
                        Style::default().add_modifier(Modifier::BOLD),
                    ))
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
            .highlight_symbol("» ");

        f.render_stateful_widget(edges_list, cols[1], &mut self.edge_state);
    }

    fn render_query_input(&self, f: &mut Frame<'_>, area: Rect) {
        let prompt = match self.mode {
            Mode::Query => "▶ ",
            Mode::Normal => "  ",
        };
        let placeholder_color = if self.query_input.is_empty() && self.mode == Mode::Normal {
            Color::DarkGray
        } else {
            Color::White
        };
        let body = if self.query_input.is_empty() && self.mode == Mode::Normal {
            r#"press ":" to type a query (e.g. FIND NEIGHBORS OF "alice")"#.to_string()
        } else {
            format!(
                "{}{}",
                self.query_input,
                if self.mode == Mode::Query { "_" } else { "" }
            )
        };

        let line = Line::from(vec![
            Span::styled(prompt, Style::default().fg(Color::Cyan)),
            Span::styled(body, Style::default().fg(placeholder_color)),
        ]);

        let title = if self.mode == Mode::Query {
            " Query · Enter to run · Esc to cancel "
        } else {
            " Query "
        };

        let border_color = if self.mode == Mode::Query {
            Color::Cyan
        } else {
            Color::DarkGray
        };

        let block = Block::default()
            .title(Span::styled(
                title,
                Style::default().add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border_color));
        let p = Paragraph::new(line).block(block).wrap(Wrap { trim: false });
        f.render_widget(p, area);
    }

    fn render_results(&self, f: &mut Frame<'_>, area: Rect) {
        let lines: Vec<Line<'_>> = if self.query_lines.is_empty() {
            let hint = if self.query_status.is_empty() {
                "(no results yet — run a query above with `:`)".to_string()
            } else {
                self.query_status.clone()
            };
            vec![Line::from(Span::styled(
                hint,
                Style::default().fg(Color::DarkGray),
            ))]
        } else {
            self.query_lines
                .iter()
                .map(|l| Line::from(Span::raw(l.as_str())))
                .collect()
        };

        let title = if self.query_status.is_empty() {
            " Results ".to_string()
        } else {
            format!(" Results · {} ", self.query_status)
        };

        let block = Block::default()
            .title(Span::styled(
                title,
                Style::default().add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::DarkGray));

        let p = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false });
        f.render_widget(p, area);
    }

    fn render_footer(&self, f: &mut Frame<'_>, area: Rect) {
        let mode_label = match self.mode {
            Mode::Normal => " NORMAL ",
            Mode::Query => " QUERY ",
        };
        let mode_color = match self.mode {
            Mode::Normal => Color::Cyan,
            Mode::Query => Color::Green,
        };
        let line = Line::from(vec![
            Span::styled(
                mode_label,
                Style::default()
                    .bg(mode_color)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                "j/k navigate · : query · +/- day · [/] week · n now · r reload · ? help · q quit",
                Style::default().fg(Color::DarkGray),
            ),
        ]);
        let p = Paragraph::new(line).alignment(Alignment::Left);
        f.render_widget(p, area);
    }

    fn render_help_overlay(&self, f: &mut Frame<'_>) {
        let area = centered_rect(60, 70, f.area());
        f.render_widget(Clear, area);

        let lines = vec![
            Line::from(Span::styled(
                "  CronaDB · keybindings  ",
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .fg(Color::Cyan),
            )),
            Line::from(""),
            Line::from(vec![Span::styled(
                "  Navigation",
                Style::default().add_modifier(Modifier::BOLD),
            )]),
            Line::from("    j / ↓        next node"),
            Line::from("    k / ↑        previous node"),
            Line::from("    g            jump to first node"),
            Line::from("    G            jump to last node"),
            Line::from("    r            reload data from disk"),
            Line::from(""),
            Line::from(vec![Span::styled(
                "  Time travel",
                Style::default().add_modifier(Modifier::BOLD),
            )]),
            Line::from("    + / -        ±1 day"),
            Line::from("    ] / [        ±7 days"),
            Line::from("    } / {        ±30 days  (Shift+] / Shift+[)"),
            Line::from("    n            reset to now (live)"),
            Line::from(""),
            Line::from(vec![Span::styled(
                "  Query",
                Style::default().add_modifier(Modifier::BOLD),
            )]),
            Line::from("    : / /        focus query box"),
            Line::from("    Enter        run query"),
            Line::from("    Esc          back to navigation"),
            Line::from(""),
            Line::from(vec![Span::styled(
                "  Other",
                Style::default().add_modifier(Modifier::BOLD),
            )]),
            Line::from("    ?            toggle this help"),
            Line::from("    q / Esc      quit"),
            Line::from(""),
            Line::from(Span::styled(
                "  press any key to dismiss",
                Style::default().fg(Color::DarkGray),
            )),
        ];

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan));
        let p = Paragraph::new(lines).block(block);
        f.render_widget(p, area);
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

fn short_ts(t: Ts) -> String {
    // Strip the time component and `Z` for compactness in the header.
    let full = t.to_rfc3339();
    full.chars().take(10).collect()
}

fn format_edge_line(e: &EdgeView) -> Line<'static> {
    let valid_to = match e.valid_to {
        Some(t) => format!("{})", short_ts(t)),
        None => ")".to_string(),
    };
    let valid = format!("[{}..{}", short_ts(e.valid_from), valid_to);

    let conf_color = if e.confidence >= 0.9 {
        Color::Green
    } else if e.confidence >= 0.7 {
        Color::Yellow
    } else {
        Color::Red
    };
    let arrow_style = if e.valid_to.is_none() {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    Line::from(vec![
        Span::styled(" → ", arrow_style),
        Span::styled(
            format!("{:<14}", truncate(&e.to_ext_id, 14)),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {:<12}", e.edge_type),
            Style::default().fg(Color::Magenta),
        ),
        Span::styled(format!(" {:<28}", valid), Style::default().fg(Color::White)),
        Span::raw(" "),
        Span::styled(
            format!("conf={:.2}", e.confidence),
            Style::default().fg(conf_color),
        ),
        Span::styled(
            if e.source.is_empty() {
                String::new()
            } else {
                format!("  src={}", e.source)
            },
            Style::default().fg(Color::DarkGray),
        ),
    ])
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
