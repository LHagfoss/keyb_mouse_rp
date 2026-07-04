use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Terminal,
};
use std::fs::File;
use std::io::Read;
use std::time::Duration;

struct MacroInfo {
    name: String,
    path: std::path::PathBuf,
    modified_str: String,
    size_kb: f64,
}

struct MacroAnalysis {
    total_events: usize,
    duration_secs: f64,
    mouse_moves: usize,
    mouse_clicks: usize,
    key_presses: usize,
    top_keys: Vec<(u16, usize)>,
}

struct TuiApp {
    macros: Vec<MacroInfo>,
    selected_index: usize,
    delay_ms: i64,
    speed: f64,
    show_delay_input: bool,
    show_speed_input: bool,
    input_value: String,
    status_msg: String,
    should_quit: bool,
    cached_analysis: Option<MacroAnalysis>,
}

impl TuiApp {
    fn new() -> Self {
        let mut app = Self {
            macros: Vec::new(),
            selected_index: 0,
            delay_ms: 0,
            speed: 1.0,
            show_delay_input: false,
            show_speed_input: false,
            input_value: String::new(),
            status_msg: "Welcome to kmrp! Select a macro and press [P] to play or [R] to record.".to_string(),
            should_quit: false,
            cached_analysis: None,
        };
        app.refresh_macros();
        app
    }

    fn refresh_macros(&mut self) {
        self.macros.clear();
        let dir = crate::storage::get_macro_dir();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            let mut temp_files = Vec::new();
            for entry in entries {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("json") {
                        if let Ok(metadata) = entry.metadata() {
                            let modified = metadata.modified().ok();
                            let size = metadata.len();
                            temp_files.push((path, modified, size));
                        }
                    }
                }
            }
            
            // Sort by modified time descending
            temp_files.sort_by(|a, b| b.1.cmp(&a.1));
            
            for (path, modified, size) in temp_files {
                let name = path.file_stem().unwrap().to_string_lossy().into_owned();
                let modified_str = if let Some(m) = modified {
                    let datetime: chrono::DateTime<chrono::Local> = m.into();
                    datetime.format("%Y-%m-%d %H:%M:%S").to_string()
                } else {
                    "Unknown".to_string()
                };
                let size_kb = size as f64 / 1024.0;
                self.macros.push(MacroInfo {
                    name,
                    path,
                    modified_str,
                    size_kb,
                });
            }
        }
        
        if self.selected_index >= self.macros.len() && !self.macros.is_empty() {
            self.selected_index = self.macros.len() - 1;
        }
    }

    fn update_analysis(&mut self) {
        self.cached_analysis = None;
        if self.macros.is_empty() || self.selected_index >= self.macros.len() {
            return;
        }
        
        let path = &self.macros[self.selected_index].path;
        if let Ok(mut file) = File::open(path) {
            let mut contents = String::new();
            if file.read_to_string(&mut contents).is_ok() {
                if let Ok(events) = serde_json::from_str::<Vec<crate::playback::RecordedEvent>>(&contents) {
                    if events.is_empty() {
                        return;
                    }
                    let total_events = events.len();
                    
                    let mut sorted = events.clone();
                    sorted.sort_by_key(|e| e.time_us);
                    let duration_secs = (sorted.last().unwrap().time_us - sorted.first().unwrap().time_us) as f64 / 1_000_000.0;
                    
                    let mut mouse_moves = 0;
                    let mut mouse_clicks = 0;
                    let mut key_presses = 0;
                    
                    use std::collections::HashMap;
                    let mut key_counts = HashMap::new();
                    
                    for ev in &events {
                        if ev.event_type == 2 { // EV_REL
                            mouse_moves += 1;
                        } else if ev.event_type == 1 { // EV_KEY
                            let is_mouse_btn = ev.code >= 272 && ev.code <= 287;
                            if is_mouse_btn {
                                if ev.value == 1 {
                                    mouse_clicks += 1;
                                }
                            } else {
                                if ev.value == 1 {
                                    key_presses += 1;
                                    *key_counts.entry(ev.code).or_insert(0) += 1;
                                }
                            }
                        }
                    }
                    
                    let mut top_keys: Vec<(u16, usize)> = key_counts.into_iter().collect();
                    top_keys.sort_by(|a, b| b.1.cmp(&a.1));
                    top_keys.truncate(3);
                    
                    self.cached_analysis = Some(MacroAnalysis {
                        total_events,
                        duration_secs,
                        mouse_moves,
                        mouse_clicks,
                        key_presses,
                        top_keys,
                    });
                }
            }
        }
    }
}

pub fn run_tui() -> Result<(), Box<dyn std::error::Error>> {
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen, crossterm::event::EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = TuiApp::new();
    app.update_analysis();

    loop {
        terminal.draw(|f| draw_ui(f, &app))?;

        if crossterm::event::poll(Duration::from_millis(100))? {
            if let crossterm::event::Event::Key(key) = crossterm::event::read()? {
                if key.kind == crossterm::event::KeyEventKind::Press {
                    if app.show_delay_input || app.show_speed_input {
                        match key.code {
                            crossterm::event::KeyCode::Enter => {
                                if app.show_delay_input {
                                    if let Ok(val) = app.input_value.parse::<i64>() {
                                        app.delay_ms = val;
                                        app.status_msg = format!("Delay offset set to {} ms", val);
                                    } else {
                                        app.status_msg = "Invalid delay integer value.".to_string();
                                    }
                                    app.show_delay_input = false;
                                } else if app.show_speed_input {
                                    if let Ok(val) = app.input_value.parse::<f64>() {
                                        if val > 0.0 {
                                            app.speed = val;
                                            app.status_msg = format!("Playback speed scale set to {}x", val);
                                        } else {
                                            app.status_msg = "Speed must be greater than 0.".to_string();
                                        }
                                    } else {
                                        app.status_msg = "Invalid speed decimal value.".to_string();
                                    }
                                    app.show_speed_input = false;
                                }
                            }
                            crossterm::event::KeyCode::Esc => {
                                app.show_delay_input = false;
                                app.show_speed_input = false;
                            }
                            crossterm::event::KeyCode::Backspace => {
                                app.input_value.pop();
                            }
                            crossterm::event::KeyCode::Char(c) => {
                                app.input_value.push(c);
                            }
                            _ => {}
                        }
                    } else {
                        match key.code {
                            crossterm::event::KeyCode::Char('q') | crossterm::event::KeyCode::Char('Q') => {
                                app.should_quit = true;
                            }
                            crossterm::event::KeyCode::Up => {
                                if app.selected_index > 0 {
                                    app.selected_index -= 1;
                                    app.update_analysis();
                                }
                            }
                            crossterm::event::KeyCode::Down => {
                                if !app.macros.is_empty() && app.selected_index < app.macros.len() - 1 {
                                    app.selected_index += 1;
                                    app.update_analysis();
                                }
                            }
                            crossterm::event::KeyCode::Char('r') | crossterm::event::KeyCode::Char('R') => {
                                crossterm::terminal::disable_raw_mode()?;
                                crossterm::execute!(terminal.backend_mut(), crossterm::terminal::LeaveAlternateScreen, crossterm::event::DisableMouseCapture)?;
                                
                                println!("Starting record module. Enter a macro name (press Enter for timestamp default):");
                                let mut name_input = String::new();
                                std::io::stdin().read_line(&mut name_input).ok();
                                let name_trimmed = name_input.trim().to_string();
                                let name_opt = if name_trimmed.is_empty() { None } else { Some(name_trimmed) };
                                
                                crate::record::record_macro(name_opt, false, false);
                                
                                println!("\nPress Enter to return to TUI dashboard...");
                                let mut temp = String::new();
                                std::io::stdin().read_line(&mut temp).ok();

                                crossterm::terminal::enable_raw_mode()?;
                                crossterm::execute!(terminal.backend_mut(), crossterm::terminal::EnterAlternateScreen, crossterm::event::EnableMouseCapture)?;
                                terminal.clear()?;
                                
                                app.refresh_macros();
                                app.update_analysis();
                            }
                            crossterm::event::KeyCode::Char('p') | crossterm::event::KeyCode::Char('P') => {
                                if !app.macros.is_empty() && app.selected_index < app.macros.len() {
                                    let macro_name = app.macros[app.selected_index].name.clone();
                                    
                                    crossterm::terminal::disable_raw_mode()?;
                                    crossterm::execute!(terminal.backend_mut(), crossterm::terminal::LeaveAlternateScreen, crossterm::event::DisableMouseCapture)?;
                                    
                                    crate::playback::play_macro(Some(macro_name), app.delay_ms, app.speed, false, false);
                                    
                                    println!("\nPress Enter to return to TUI dashboard...");
                                    let mut temp = String::new();
                                    std::io::stdin().read_line(&mut temp).ok();

                                    crossterm::terminal::enable_raw_mode()?;
                                    crossterm::execute!(terminal.backend_mut(), crossterm::terminal::EnterAlternateScreen, crossterm::event::EnableMouseCapture)?;
                                    terminal.clear()?;
                                    
                                    app.refresh_macros();
                                    app.update_analysis();
                                }
                            }
                            crossterm::event::KeyCode::Char('d') | crossterm::event::KeyCode::Char('D') => {
                                app.show_delay_input = true;
                                app.input_value = app.delay_ms.to_string();
                            }
                            crossterm::event::KeyCode::Char('s') | crossterm::event::KeyCode::Char('S') => {
                                app.show_speed_input = true;
                                app.input_value = app.speed.to_string();
                            }
                            crossterm::event::KeyCode::Char('l') | crossterm::event::KeyCode::Char('L') => {
                                if !app.macros.is_empty() && app.selected_index < app.macros.len() {
                                    let macro_name = app.macros[app.selected_index].name.clone();
                                    let path = app.macros[app.selected_index].path.clone();
                                    
                                    if std::fs::remove_file(&path).is_ok() {
                                        app.status_msg = format!("Deleted macro '{}'", macro_name);
                                    } else {
                                        app.status_msg = format!("Failed to delete macro '{}'", macro_name);
                                    }
                                    
                                    app.refresh_macros();
                                    app.update_analysis();
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(terminal.backend_mut(), crossterm::terminal::LeaveAlternateScreen, crossterm::event::DisableMouseCapture)?;
    terminal.show_cursor()?;

    Ok(())
}

fn draw_ui(f: &mut ratatui::Frame, app: &TuiApp) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header / logo banner
            Constraint::Min(10),   // Main dashboard (split)
            Constraint::Length(3), // Status bar
            Constraint::Length(3), // Keybind help bar
        ])
        .split(f.area());

    let header = Paragraph::new(" KMRP - HIGH PRECISION INPUT MACRO DASHBOARD ")
        .style(Style::default().fg(Color::Cyan).bold())
        .alignment(ratatui::layout::Alignment::Center)
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan)));
    f.render_widget(header, chunks[0]);

    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40), // Left: List
            Constraint::Percentage(60), // Right: Details/Analysis
        ])
        .split(chunks[1]);

    let mut list_items = Vec::new();
    for (i, m) in app.macros.iter().enumerate() {
        let style = if i == app.selected_index {
            Style::default().fg(Color::Yellow).bold().bg(Color::Rgb(40, 40, 40))
        } else {
            Style::default().fg(Color::White)
        };
        list_items.push(ListItem::new(format!(" 📁 {:<24} [{:>8.2} KB]", m.name, m.size_kb)).style(style));
    }
    
    let list_title = format!(" Macros ({}) ", app.macros.len());
    let list_block = Block::default()
        .title(list_title.bold().cyan())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let list = List::new(list_items).block(list_block);
    f.render_widget(list, main_chunks[0]);

    let details_block = Block::default()
        .title(" Selected Macro Details & Analysis ".bold().cyan())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
        
    let mut details_lines = Vec::new();
    if app.macros.is_empty() || app.selected_index >= app.macros.len() {
        details_lines.push(Line::from("No macros saved yet. Press [R] to record a new macro."));
    } else {
        let sel = &app.macros[app.selected_index];
        details_lines.push(Line::from(vec![
            Span::raw("Name: ").bold().cyan(),
            Span::raw(&sel.name).yellow().bold(),
        ]));
        details_lines.push(Line::from(vec![
            Span::raw("Path: ").cyan(),
            Span::raw(sel.path.to_string_lossy().to_string()).dark_gray(),
        ]));
        details_lines.push(Line::from(vec![
            Span::raw("Modified: ").cyan(),
            Span::raw(&sel.modified_str),
        ]));
        details_lines.push(Line::from(vec![
            Span::raw("File Size: ").cyan(),
            Span::raw(format!("{:.2} KB", sel.size_kb)),
        ]));
        
        details_lines.push(Line::from(""));
        details_lines.push(Line::from("───── ANALYSIS ──────────────────────────────────────".cyan()));
        
        if let Some(analysis) = &app.cached_analysis {
            details_lines.push(Line::from(vec![
                Span::raw("Total Events: ").cyan(),
                Span::raw(analysis.total_events.to_string()).bold().green(),
            ]));
            
            let duration_min = (analysis.duration_secs / 60.0).floor() as u32;
            let duration_sec = (analysis.duration_secs % 60.0).round() as u32;
            details_lines.push(Line::from(vec![
                Span::raw("Playback Duration: ").cyan(),
                Span::raw(format!("{:02}:{:02}", duration_min, duration_sec)).bold().yellow(),
                Span::raw(format!(" ({:.2} seconds)", analysis.duration_secs)).dark_gray(),
            ]));
            
            details_lines.push(Line::from(vec![
                Span::raw("Mouse Move Events (EV_REL): ").cyan(),
                Span::raw(analysis.mouse_moves.to_string()).bold().magenta(),
            ]));
            details_lines.push(Line::from(vec![
                Span::raw("Mouse Clicks (BTN_MOUSE): ").cyan(),
                Span::raw(analysis.mouse_clicks.to_string()).bold().magenta(),
            ]));
            details_lines.push(Line::from(vec![
                Span::raw("Keyboard Keypresses: ").cyan(),
                Span::raw(analysis.key_presses.to_string()).bold().blue(),
            ]));
            
            if !analysis.top_keys.is_empty() {
                details_lines.push(Line::from(""));
                details_lines.push(Line::from("Top Keyboard Keys Pressed:".cyan().bold()));
                for (i, (code, count)) in analysis.top_keys.iter().enumerate() {
                    let key_name = match code {
                        1 => "ESC",
                        16 => "Q",
                        30 => "A",
                        31 => "S",
                        32 => "D",
                        44 => "Z",
                        45 => "X",
                        57 => "SPACE",
                        28 => "ENTER",
                        _ => "OTHER",
                    };
                    details_lines.push(Line::from(format!("  {}. KeyCode {} ({}) pressed {} times", i + 1, code, key_name, count)));
                }
            }
        } else {
            details_lines.push(Line::from("Parsing and analyzing macro file...".dark_gray()));
        }
        
        details_lines.push(Line::from(""));
        details_lines.push(Line::from("───── REPLAY SETTINGS ───────────────────────────────".cyan()));
        details_lines.push(Line::from(vec![
            Span::raw("Speed Multiplier: ").cyan(),
            Span::raw(format!("{:.4}x", app.speed)).bold().green(),
            Span::raw("  (Press [S] to change)").dark_gray(),
        ]));
        details_lines.push(Line::from(vec![
            Span::raw("Timeline Delay Shift: ").cyan(),
            Span::raw(format!("{} ms", app.delay_ms)).bold().green(),
            Span::raw("  (Press [D] to change)").dark_gray(),
        ]));
    }
    
    let details = Paragraph::new(details_lines).block(details_block).wrap(Wrap { trim: true });
    f.render_widget(details, main_chunks[1]);

    let status_block = Block::default()
        .title(" Status ".bold().cyan())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let status = Paragraph::new(app.status_msg.clone()).block(status_block);
    f.render_widget(status, chunks[2]);

    let help_block = Block::default()
        .title(" Controls / Keybinds ".bold().cyan())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let help = Paragraph::new(" [▲/▼] Navigate | [P] Play Select | [R] Record New | [S] Set Speed | [D] Set Delay | [L] Delete | [Q] Quit")
        .style(Style::default().fg(Color::DarkGray))
        .block(help_block);
    f.render_widget(help, chunks[3]);

    if app.show_delay_input || app.show_speed_input {
        let popup_title = if app.show_delay_input { " Set Timeline Delay (ms) " } else { " Set Playback Speed (multiplier) " };
        let popup_text = format!("Current input: {}\n\nPress [Enter] to Save | [Esc] to Cancel", app.input_value);
        
        let area = centered_rect(60, 20, f.area());
        f.render_widget(ratatui::widgets::Clear, area);
        
        let popup = Paragraph::new(popup_text)
            .block(Block::default()
                .title(popup_title.bold().yellow())
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow))
            )
            .alignment(ratatui::layout::Alignment::Center);
        f.render_widget(popup, area);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: ratatui::layout::Rect) -> ratatui::layout::Rect {
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
