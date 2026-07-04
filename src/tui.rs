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
use std::sync::atomic::{AtomicBool, Ordering, AtomicUsize};
use std::sync::{Arc, Mutex};
use std::thread;
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

enum AppState {
    Dashboard,
    InputMacroName,
    RecordingCountdown {
        seconds_left: u32,
        last_tick: std::time::Instant,
        macro_name: String,
    },
    Recording {
        start_time: std::time::Instant,
        event_count: Arc<AtomicUsize>,
        events: Arc<Mutex<Vec<crate::playback::RecordedEvent>>>,
        recording_flag: Arc<AtomicBool>,
        macro_name: String,
    },
    PlayingCountdown {
        seconds_left: u32,
        last_tick: std::time::Instant,
        macro_name: String,
    },
    Playing {
        start_time: std::time::Instant,
        current_event: Arc<AtomicUsize>,
        total_events: usize,
        playing_flag: Arc<AtomicBool>,
        aborted_flag: Arc<AtomicBool>,
        macro_name: String,
        _handle: thread::JoinHandle<()>,
    },
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
    state: AppState,
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
            state: AppState::Dashboard,
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

        if crossterm::event::poll(Duration::from_millis(50))? {
            if let crossterm::event::Event::Key(key) = crossterm::event::read()? {
                if key.kind == crossterm::event::KeyEventKind::Press {
                    match &mut app.state {
                        AppState::Dashboard => {
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
                                        if c.is_digit(10) || c == '-' || c == '.' {
                                            app.input_value.push(c);
                                        }
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
                                        app.state = AppState::InputMacroName;
                                        app.input_value = String::new();
                                        app.status_msg = "Enter macro name and press [Enter].".to_string();
                                    }
                                    crossterm::event::KeyCode::Char('p') | crossterm::event::KeyCode::Char('P') => {
                                        if !app.macros.is_empty() && app.selected_index < app.macros.len() {
                                            let macro_name = app.macros[app.selected_index].name.clone();
                                            app.state = AppState::PlayingCountdown {
                                                seconds_left: 3,
                                                last_tick: std::time::Instant::now(),
                                                macro_name,
                                            };
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
                        AppState::InputMacroName => {
                            match key.code {
                                crossterm::event::KeyCode::Enter => {
                                    let name_trimmed = app.input_value.trim().to_string();
                                    let macro_name = if name_trimmed.is_empty() {
                                        let datetime = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
                                        format!("macro_{}", datetime)
                                    } else {
                                        name_trimmed
                                    };
                                    app.state = AppState::RecordingCountdown {
                                        seconds_left: 3,
                                        last_tick: std::time::Instant::now(),
                                        macro_name,
                                    };
                                    app.status_msg = "Preparing background threads for recording...".to_string();
                                }
                                crossterm::event::KeyCode::Esc => {
                                    app.state = AppState::Dashboard;
                                    app.status_msg = "Cancelled naming. Returned to dashboard.".to_string();
                                }
                                crossterm::event::KeyCode::Backspace => {
                                    app.input_value.pop();
                                }
                                crossterm::event::KeyCode::Char(c) => {
                                    if c.is_alphanumeric() || c == '_' || c == '-' {
                                        app.input_value.push(c);
                                    }
                                }
                                _ => {}
                            }
                        }
                        AppState::Recording { .. } => {
                            match key.code {
                                crossterm::event::KeyCode::Esc | crossterm::event::KeyCode::Char('q') | crossterm::event::KeyCode::Char('Q') => {
                                    let old_state = std::mem::replace(&mut app.state, AppState::Dashboard);
                                    if let AppState::Recording { events, recording_flag, macro_name, .. } = old_state {
                                        recording_flag.store(false, Ordering::SeqCst);

                                        let mut events_lock = events.lock().unwrap();
                                        events_lock.sort_by_key(|e| e.time_us);
                                        crate::record::trim_exit_events(&mut events_lock);

                                        let save_path = crate::storage::get_macro_path(&macro_name);
                                        crate::record::save_and_exit(&events_lock, &save_path);

                                        app.status_msg = format!("Macro '{}' saved successfully!", macro_name);
                                        app.refresh_macros();
                                        app.update_analysis();
                                    }
                                }
                                _ => {}
                            }
                        }
                        AppState::Playing { aborted_flag, .. } => {
                            match key.code {
                                crossterm::event::KeyCode::Esc | crossterm::event::KeyCode::Char('q') | crossterm::event::KeyCode::Char('Q') => {
                                    aborted_flag.store(true, Ordering::SeqCst);
                                    app.status_msg = "Playback abort requested by user.".to_string();
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Tick logic
        let mut next_state = None;

        match &mut app.state {
            AppState::RecordingCountdown { seconds_left, last_tick, macro_name } => {
                if last_tick.elapsed() >= Duration::from_secs(1) {
                    *seconds_left -= 1;
                    *last_tick = std::time::Instant::now();
                    if *seconds_left == 0 {
                        match crate::record::start_background_recording(false, false) {
                            Ok((events, recording_flag, _handles)) => {
                                next_state = Some((
                                    AppState::Recording {
                                        start_time: std::time::Instant::now(),
                                        event_count: Arc::new(AtomicUsize::new(0)),
                                        events,
                                        recording_flag,
                                        macro_name: macro_name.clone(),
                                    },
                                    format!("Recording macro '{}'...", macro_name)
                                ));
                            }
                            Err(e) => {
                                next_state = Some((
                                    AppState::Dashboard,
                                    format!("Recording error: {}", e)
                                ));
                            }
                        }
                    }
                }
            }
            AppState::PlayingCountdown { seconds_left, last_tick, macro_name } => {
                if last_tick.elapsed() >= Duration::from_secs(1) {
                    *seconds_left -= 1;
                    *last_tick = std::time::Instant::now();
                    if *seconds_left == 0 {
                        match crate::playback::start_background_playback(macro_name.clone(), app.delay_ms, app.speed, false, false) {
                            Ok((playing_flag, current_event, aborted_flag, total_events, handle)) => {
                                next_state = Some((
                                    AppState::Playing {
                                        start_time: std::time::Instant::now(),
                                        current_event,
                                        total_events,
                                        playing_flag,
                                        aborted_flag,
                                        macro_name: macro_name.clone(),
                                        _handle: handle,
                                    },
                                    format!("Playing macro '{}'...", macro_name)
                                ));
                            }
                            Err(e) => {
                                next_state = Some((
                                    AppState::Dashboard,
                                    format!("Playback error: {}", e)
                                ));
                            }
                        }
                    }
                }
            }
            AppState::Recording { event_count, events, .. } => {
                if let Ok(lock) = events.lock() {
                    event_count.store(lock.len(), Ordering::SeqCst);
                }
            }
            AppState::Playing { playing_flag, aborted_flag, .. } => {
                if !playing_flag.load(Ordering::SeqCst) {
                    let aborted = aborted_flag.load(Ordering::SeqCst);
                    let msg = if aborted {
                        "Playback aborted by user.".to_string()
                    } else {
                        "Playback finished successfully.".to_string()
                    };
                    next_state = Some((AppState::Dashboard, msg));
                }
            }
            _ => {}
        }

        if let Some((state, status)) = next_state {
            app.state = state;
            app.status_msg = status;
            if let AppState::Dashboard = app.state {
                app.refresh_macros();
                app.update_analysis();
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
    match &app.state {
        AppState::Dashboard | AppState::InputMacroName => {
            draw_dashboard(f, app);
        }
        AppState::RecordingCountdown { seconds_left, .. } => {
            draw_countdown_screen(f, "RECORDING COUNTDOWN", *seconds_left);
        }
        AppState::Recording { start_time, event_count, macro_name, .. } => {
            draw_recording_screen(f, start_time.elapsed(), event_count.load(Ordering::SeqCst), macro_name);
        }
        AppState::PlayingCountdown { seconds_left, .. } => {
            draw_countdown_screen(f, "PLAYBACK COUNTDOWN", *seconds_left);
        }
        AppState::Playing { start_time, current_event, total_events, macro_name, .. } => {
            draw_playback_screen(f, start_time.elapsed(), current_event.load(Ordering::SeqCst), *total_events, macro_name, app);
        }
    }
}

fn draw_dashboard(f: &mut ratatui::Frame, app: &TuiApp) {
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
            Span::raw(sel.path.to_string_lossy().to_string()).white(),
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
                        2 => "1",
                        3 => "2",
                        4 => "3",
                        5 => "4",
                        6 => "5",
                        7 => "6",
                        8 => "7",
                        9 => "8",
                        10 => "9",
                        11 => "0",
                        12 => "MINUS",
                        13 => "EQUAL",
                        14 => "BACKSPACE",
                        15 => "TAB",
                        16 => "Q",
                        17 => "W",
                        18 => "E",
                        19 => "R",
                        20 => "T",
                        21 => "Y",
                        22 => "U",
                        23 => "I",
                        24 => "O",
                        25 => "P",
                        26 => "LEFTBRACE",
                        27 => "RIGHTBRACE",
                        28 => "ENTER",
                        29 => "LEFTCTRL",
                        30 => "A",
                        31 => "S",
                        32 => "D",
                        33 => "F",
                        34 => "G",
                        35 => "H",
                        36 => "J",
                        37 => "K",
                        38 => "L",
                        39 => "SEMICOLON",
                        40 => "APOSTROPHE",
                        41 => "GRAVE",
                        42 => "LEFTSHIFT",
                        43 => "BACKSLASH",
                        44 => "Z",
                        45 => "X",
                        46 => "C",
                        47 => "V",
                        48 => "B",
                        49 => "N",
                        50 => "M",
                        51 => "COMMA",
                        52 => "DOT",
                        53 => "SLASH",
                        54 => "RIGHTSHIFT",
                        55 => "KPASTERISK",
                        56 => "LEFTALT",
                        57 => "SPACE",
                        58 => "CAPSLOCK",
                        59 => "F1",
                        60 => "F2",
                        61 => "F3",
                        62 => "F4",
                        63 => "F5",
                        64 => "F6",
                        65 => "F7",
                        66 => "F8",
                        67 => "F9",
                        68 => "F10",
                        _ => "OTHER",
                    };
                    details_lines.push(Line::from(format!("  {}. Key Code {} ({}) pressed {} times", i + 1, code, key_name, count)));
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
            Span::raw("  (Press ").yellow(),
            Span::raw("[S]").green().bold(),
            Span::raw(" to change)").yellow(),
        ]));
        details_lines.push(Line::from(vec![
            Span::raw("Timeline Delay Shift: ").cyan(),
            Span::raw(format!("{} ms", app.delay_ms)).bold().green(),
            Span::raw("  (Press ").yellow(),
            Span::raw("[D]").green().bold(),
            Span::raw(" to change)").yellow(),
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
    
    let help_line = Line::from(vec![
        Span::raw(" Navigate: ").cyan().bold(),
        Span::raw("[▲/▼]").yellow().bold(),
        Span::raw(" | Play: ").cyan().bold(),
        Span::raw("[P]").yellow().bold(),
        Span::raw(" | Record: ").cyan().bold(),
        Span::raw("[R]").yellow().bold(),
        Span::raw(" | Speed: ").cyan().bold(),
        Span::raw("[S]").yellow().bold(),
        Span::raw(" | Delay: ").cyan().bold(),
        Span::raw("[D]").yellow().bold(),
        Span::raw(" | Delete: ").cyan().bold(),
        Span::raw("[L]").yellow().bold(),
        Span::raw(" | Quit: ").cyan().bold(),
        Span::raw("[Q]").yellow().bold(),
    ]);

    let help = Paragraph::new(help_line).block(help_block);
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

    if let AppState::InputMacroName = app.state {
        let area = centered_rect(60, 20, f.area());
        f.render_widget(ratatui::widgets::Clear, area);
        
        let popup_text = format!("Enter macro name: {}\n\n(Press [Enter] to Start Countdown | [Esc] to Cancel)", app.input_value);
        let popup = Paragraph::new(popup_text)
            .block(Block::default()
                .title(" Record New Macro: Name Input ".bold().yellow())
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow))
            )
            .alignment(ratatui::layout::Alignment::Center);
        f.render_widget(popup, area);
    }
}

fn draw_countdown_screen(f: &mut ratatui::Frame, title: &str, seconds_left: u32) {
    let area = centered_rect(50, 30, f.area());
    f.render_widget(ratatui::widgets::Clear, area);

    let text = format!(
        "\n\nStarting in...\n\n\n [ {} ]",
        seconds_left.to_string().magenta().bold()
    );

    let block = Block::default()
        .title(format!(" {} ", title).bold().cyan())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let paragraph = Paragraph::new(text)
        .block(block)
        .alignment(ratatui::layout::Alignment::Center);

    f.render_widget(paragraph, area);
}

fn draw_recording_screen(f: &mut ratatui::Frame, elapsed: Duration, count: usize, name: &str) {
    let area = centered_rect(60, 40, f.area());
    f.render_widget(ratatui::widgets::Clear, area);

    let elapsed_secs = elapsed.as_secs();
    let min = elapsed_secs / 60;
    let sec = elapsed_secs % 60;

    let text = vec![
        Line::from(""),
        Line::from(vec![
            Span::raw(" Recording Macro: ").cyan(),
            Span::raw(name).yellow().bold(),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::raw(" Elapsed Time: ").cyan(),
            Span::raw(format!("{:02}:{:02}", min, sec)).bold().yellow(),
        ]),
        Line::from(vec![
            Span::raw(" Recorded Events: ").cyan(),
            Span::raw(count.to_string()).bold().green(),
        ]),
        Line::from(""),
        Line::from("──────────────────────────────────────────────────".cyan()),
        Line::from(""),
        Line::from(" Press [Esc] or [Q] to Stop and Save ".red().bold()),
    ];

    let block = Block::default()
        .title(" 🔴 RECORDING ACTIVE ".bold().red())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red));

    let paragraph = Paragraph::new(text)
        .block(block)
        .alignment(ratatui::layout::Alignment::Center);

    f.render_widget(paragraph, area);
}

fn draw_playback_screen(f: &mut ratatui::Frame, elapsed: Duration, current: usize, total: usize, name: &str, app: &TuiApp) {
    let area = centered_rect(70, 45, f.area());
    f.render_widget(ratatui::widgets::Clear, area);

    let elapsed_secs = elapsed.as_secs();
    let min = elapsed_secs / 60;
    let sec = elapsed_secs % 60;

    let pct = if total > 0 {
        ((current as f64 / total as f64) * 100.0).min(100.0) as u16
    } else {
        0
    };

    let gauge = ratatui::widgets::Gauge::default()
        .block(Block::default().borders(Borders::NONE))
        .gauge_style(Style::default().fg(Color::Green).bg(Color::Rgb(50, 50, 50)).bold())
        .percent(pct)
        .label(format!("{}% ({}/{})", pct, current, total));

    let text_lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::raw(" Replaying Macro: ").cyan(),
            Span::raw(name).yellow().bold(),
        ]),
        Line::from(vec![
            Span::raw(" Elapsed Time: ").cyan(),
            Span::raw(format!("{:02}:{:02}", min, sec)).bold().yellow(),
        ]),
        Line::from(vec![
            Span::raw(" Speed: ").cyan(),
            Span::raw(format!("{:.4}x", app.speed)).bold().green(),
            Span::raw("  |  Delay: ").cyan(),
            Span::raw(format!("{} ms", app.delay_ms)).bold().green(),
        ]),
        Line::from(""),
        Line::from(""), 
        Line::from(""),
        Line::from(" Press physical ESC or Q to abort ".red().bold()),
    ];

    let block = Block::default()
        .title(" ▶ PLAYBACK ACTIVE ".bold().green())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green));

    let paragraph = Paragraph::new(text_lines)
        .block(block)
        .alignment(ratatui::layout::Alignment::Center);

    f.render_widget(paragraph, area);

    let gauge_area = ratatui::layout::Rect {
        x: area.x + 5,
        y: area.y + area.height - 4,
        width: area.width - 10,
        height: 1,
    };
    f.render_widget(gauge, gauge_area);
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
