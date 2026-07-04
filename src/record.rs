use crate::playback::RecordedEvent;
use crossterm::event::{self, Event as CrossEvent, KeyCode as CrossKeyCode, KeyEvent};
use evdev::{Device, EventType};
use std::fs::File;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, UNIX_EPOCH};

pub fn start_background_recording(
    no_mouse: bool,
    no_keyboard: bool,
) -> Result<
    (
        Arc<Mutex<Vec<RecordedEvent>>>,
        Arc<AtomicBool>,
        Vec<thread::JoinHandle<()>>,
    ),
    String,
> {
    let events = Arc::new(Mutex::new(Vec::new()));

    let mut devices = Vec::new();
    let entries =
        std::fs::read_dir("/dev/input").map_err(|e| format!("Error reading /dev/input: {}", e))?;
    let mut permission_denied = false;

    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .map_or(false, |s| s.starts_with("event"))
            {
                match Device::open(&path) {
                    Ok(device) => {
                        let supports_keys = device.supported_events().contains(EventType::KEY);
                        let supports_rel = device.supported_events().contains(EventType::RELATIVE);

                        let has_mouse_keys = device.supported_keys().map_or(false, |keys| {
                            keys.iter().any(|k| k.code() >= 272 && k.code() <= 287)
                        });
                        let is_mouse = supports_rel || has_mouse_keys;
                        let is_keyboard = supports_keys && !has_mouse_keys;

                        let mut keep = false;
                        if is_mouse && !no_mouse {
                            keep = true;
                        }
                        if is_keyboard && !no_keyboard {
                            keep = true;
                        }

                        if keep && (supports_keys || supports_rel) {
                            devices.push((path, device));
                        }
                    }
                    Err(err) => {
                        if err.kind() == std::io::ErrorKind::PermissionDenied {
                            permission_denied = true;
                        }
                    }
                }
            }
        }
    }

    if devices.is_empty() {
        if permission_denied {
            return Err("Permission denied: Cannot access input devices in /dev/input/. Please run as root or add user to group.".to_string());
        } else {
            return Err(
                "No compatible keyboard or mouse input devices found in /dev/input/.".to_string(),
            );
        }
    }

    let recording = Arc::new(AtomicBool::new(true));
    let mut handles = Vec::new();

    for (_path, mut device) in devices {
        let events_clone = Arc::clone(&events);
        let recording_clone = Arc::clone(&recording);

        let handle = thread::spawn(move || {
            while recording_clone.load(Ordering::SeqCst) {
                match device.fetch_events() {
                    Ok(events_iter) => {
                        if !recording_clone.load(Ordering::SeqCst) {
                            break;
                        }
                        let mut events_lock = events_clone.lock().unwrap();

                        for event in events_iter {
                            let is_mouse = event.event_type().0 == 2
                                || (event.event_type().0 == 1
                                    && event.code() >= 272
                                    && event.code() <= 287);
                            let is_keyboard = event.event_type().0 == 1 && !is_mouse;

                            if is_mouse && no_mouse {
                                continue;
                            }
                            if is_keyboard && no_keyboard {
                                continue;
                            }
                            if event.event_type().0 == 0 && no_mouse && no_keyboard {
                                continue;
                            }

                            let time_us = event
                                .timestamp()
                                .duration_since(UNIX_EPOCH)
                                .unwrap_or(Duration::ZERO)
                                .as_micros() as u64;

                            events_lock.push(RecordedEvent {
                                time_us,
                                event_type: event.event_type().0,
                                code: event.code(),
                                value: event.value(),
                            });
                        }
                    }
                    Err(_) => {
                        break;
                    }
                }
            }
        });
        handles.push(handle);
    }

    Ok((events, recording, handles))
}

pub fn record_macro(name: Option<String>, no_mouse: bool, no_keyboard: bool) {
    let mut permission_denied = false;
    let (events, recording, handles) = match start_background_recording(no_mouse, no_keyboard) {
        Ok(res) => res,
        Err(e) => {
            if e.contains("Permission denied") {
                permission_denied = true;
            }
            use colored::Colorize;
            if permission_denied {
                eprintln!("{}", "Error: Permission Denied".red().bold());
                eprintln!("Cannot access input devices in /dev/input/.");
                eprintln!("Please run this tool with sudo, or add your user to the 'input' group:");
                eprintln!("    sudo usermod -aG input $USER");
                eprintln!(
                    "(Note: You will need to log out and log back in for group changes to take effect.)"
                );
            } else {
                eprintln!("{}", "Error: No devices found".red().bold());
                eprintln!("{}", e);
            }
            return;
        }
    };

    use colored::Colorize;
    crate::ui::print_info_box(
        "MACRO RECORDING MODULE",
        &[
            format!(
                "{}: {}",
                "Active Devices",
                handles.len().to_string().cyan().bold()
            ),
            "".to_string(),
            format!("{}:", "How to Stop".yellow().bold()),
            "  - Focus this terminal window.".to_string(),
            "  - Press ESCAPE or Q inside the terminal window to stop.".to_string(),
            "".to_string(),
            format!(
                "{}: Initializing recording input listener...",
                "STATUS".blue().bold()
            ),
        ],
    );

    for i in (1..=3).rev() {
        println!("  [{}] Starting in {}s...", "COUNTDOWN".magenta().bold(), i);
        thread::sleep(Duration::from_secs(1));
    }
    println!();
    println!(
        "  [{}] {}",
        "STATUS".green().bold(),
        "Recording has started! Type or move mouse now.".bright_green()
    );

    crossterm::terminal::enable_raw_mode().unwrap();

    loop {
        if let Ok(true) = event::poll(Duration::from_millis(50))
            && let Ok(CrossEvent::Key(KeyEvent {
                code: CrossKeyCode::Esc | CrossKeyCode::Char('q') | CrossKeyCode::Char('Q'),
                ..
            })) = event::read()
        {
            break;
        }
    }

    crossterm::terminal::disable_raw_mode().unwrap();

    recording.store(false, Ordering::SeqCst);

    for h in handles {
        h.join().ok();
    }

    let mut events_lock = events.lock().unwrap();

    events_lock.sort_by_key(|e| e.time_us);

    trim_exit_events(&mut events_lock);

    let name_str = match name {
        Some(n) => n,
        None => {
            let datetime = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
            format!("macro_{}", datetime)
        }
    };
    let save_path = crate::storage::get_macro_path(&name_str);

    save_and_exit(&events_lock, &save_path);
}

pub fn trim_exit_events(events: &mut Vec<RecordedEvent>) {
    while let Some(last) = events.last() {
        if last.event_type == 0 {
            // EV_SYN
            events.pop();
        } else if last.event_type == 1 && (last.code == 16 || last.code == 1) {
            // KEY_Q or KEY_ESC
            events.pop();
        } else {
            break;
        }
    }
}

pub fn save_and_exit(events_lock: &Vec<RecordedEvent>, path: &std::path::Path) {
    use colored::Colorize;
    let json = match serde_json::to_string_pretty(events_lock) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("{} {}", "Error serializing macro:".red().bold(), e);
            return;
        }
    };

    let mut file = File::create(path).expect(&format!("Unable to create file: {:?}", path));
    file.write_all(json.as_bytes())
        .expect("Unable to write data");

    println!();
    println!(
        "  [{}] Saved {} events to {} successfully!",
        "SUCCESS".green().bold(),
        events_lock.len().to_string().cyan().bold(),
        path.to_string_lossy().yellow().bold()
    );
}
