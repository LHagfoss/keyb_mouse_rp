use crate::playback::RecordedEvent;
use crossterm::event::{self, Event as CrossEvent, KeyCode as CrossKeyCode, KeyEvent};
use std::fs::File;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
#[allow(unused_imports)]
use std::time::{Duration, UNIX_EPOCH};

#[cfg(target_os = "linux")]
use evdev::{Device, EventType};

#[cfg(target_os = "macos")]
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
}

#[cfg(target_os = "macos")]
pub fn is_trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

#[cfg(target_os = "macos")]
pub(crate) fn rdev_key_to_evdev(key: rdev::Key) -> u16 {
    match key {
        rdev::Key::Escape => 1,
        rdev::Key::Num1 => 2,
        rdev::Key::Num2 => 3,
        rdev::Key::Num3 => 4,
        rdev::Key::Num4 => 5,
        rdev::Key::Num5 => 6,
        rdev::Key::Num6 => 7,
        rdev::Key::Num7 => 8,
        rdev::Key::Num8 => 9,
        rdev::Key::Num9 => 10,
        rdev::Key::Num0 => 11,
        rdev::Key::Minus => 12,
        rdev::Key::Equal => 13,
        rdev::Key::Backspace => 14,
        rdev::Key::Tab => 15,
        rdev::Key::KeyQ => 16,
        rdev::Key::KeyW => 17,
        rdev::Key::KeyE => 18,
        rdev::Key::KeyR => 19,
        rdev::Key::KeyT => 20,
        rdev::Key::KeyY => 21,
        rdev::Key::KeyU => 22,
        rdev::Key::KeyI => 23,
        rdev::Key::KeyO => 24,
        rdev::Key::KeyP => 25,
        rdev::Key::LeftBracket => 26,
        rdev::Key::RightBracket => 27,
        rdev::Key::Return => 28,
        rdev::Key::ControlLeft => 29,
        rdev::Key::KeyA => 30,
        rdev::Key::KeyS => 31,
        rdev::Key::KeyD => 32,
        rdev::Key::KeyF => 33,
        rdev::Key::KeyG => 34,
        rdev::Key::KeyH => 35,
        rdev::Key::KeyJ => 36,
        rdev::Key::KeyK => 37,
        rdev::Key::KeyL => 38,
        rdev::Key::SemiColon => 39,
        rdev::Key::Quote => 40,
        rdev::Key::BackQuote => 41,
        rdev::Key::ShiftLeft => 42,
        rdev::Key::BackSlash => 43,
        rdev::Key::KeyZ => 44,
        rdev::Key::KeyX => 45,
        rdev::Key::KeyC => 46,
        rdev::Key::KeyV => 47,
        rdev::Key::KeyB => 48,
        rdev::Key::KeyN => 49,
        rdev::Key::KeyM => 50,
        rdev::Key::Comma => 51,
        rdev::Key::Dot => 52,
        rdev::Key::Slash => 53,
        rdev::Key::ShiftRight => 54,
        rdev::Key::KpMultiply => 55,
        rdev::Key::Alt => 56,
        rdev::Key::Space => 57,
        rdev::Key::CapsLock => 58,
        rdev::Key::F1 => 59,
        rdev::Key::F2 => 60,
        rdev::Key::F3 => 61,
        rdev::Key::F4 => 62,
        rdev::Key::F5 => 63,
        rdev::Key::F6 => 64,
        rdev::Key::F7 => 65,
        rdev::Key::F8 => 66,
        rdev::Key::F9 => 67,
        rdev::Key::F10 => 68,
        rdev::Key::NumLock => 69,
        rdev::Key::ScrollLock => 70,
        rdev::Key::Kp7 => 71,
        rdev::Key::Kp8 => 72,
        rdev::Key::Kp9 => 73,
        rdev::Key::KpMinus => 74,
        rdev::Key::Kp4 => 75,
        rdev::Key::Kp5 => 76,
        rdev::Key::Kp6 => 77,
        rdev::Key::KpPlus => 78,
        rdev::Key::Kp1 => 79,
        rdev::Key::Kp2 => 80,
        rdev::Key::Kp3 => 81,
        rdev::Key::Kp0 => 82,
        rdev::Key::KpDelete => 83,
        rdev::Key::F11 => 87,
        rdev::Key::F12 => 88,
        rdev::Key::KpReturn => 96,
        rdev::Key::ControlRight => 97,
        rdev::Key::KpDivide => 98,
        rdev::Key::PrintScreen => 99,
        rdev::Key::AltGr => 100,
        rdev::Key::Home => 102,
        rdev::Key::UpArrow => 103,
        rdev::Key::PageUp => 104,
        rdev::Key::LeftArrow => 105,
        rdev::Key::RightArrow => 106,
        rdev::Key::End => 107,
        rdev::Key::DownArrow => 108,
        rdev::Key::PageDown => 109,
        rdev::Key::Insert => 110,
        rdev::Key::Delete => 111,
        rdev::Key::Pause => 119,
        rdev::Key::MetaLeft => 125,
        rdev::Key::MetaRight => 126,
        _ => 0,
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn evdev_to_rdev_key(code: u16) -> Option<rdev::Key> {
    match code {
        1 => Some(rdev::Key::Escape),
        2 => Some(rdev::Key::Num1),
        3 => Some(rdev::Key::Num2),
        4 => Some(rdev::Key::Num3),
        5 => Some(rdev::Key::Num4),
        6 => Some(rdev::Key::Num5),
        7 => Some(rdev::Key::Num6),
        8 => Some(rdev::Key::Num7),
        9 => Some(rdev::Key::Num8),
        10 => Some(rdev::Key::Num9),
        11 => Some(rdev::Key::Num0),
        12 => Some(rdev::Key::Minus),
        13 => Some(rdev::Key::Equal),
        14 => Some(rdev::Key::Backspace),
        15 => Some(rdev::Key::Tab),
        16 => Some(rdev::Key::KeyQ),
        17 => Some(rdev::Key::KeyW),
        18 => Some(rdev::Key::KeyE),
        19 => Some(rdev::Key::KeyR),
        20 => Some(rdev::Key::KeyT),
        21 => Some(rdev::Key::KeyY),
        22 => Some(rdev::Key::KeyU),
        23 => Some(rdev::Key::KeyI),
        24 => Some(rdev::Key::KeyO),
        25 => Some(rdev::Key::KeyP),
        26 => Some(rdev::Key::LeftBracket),
        27 => Some(rdev::Key::RightBracket),
        28 => Some(rdev::Key::Return),
        29 => Some(rdev::Key::ControlLeft),
        30 => Some(rdev::Key::KeyA),
        31 => Some(rdev::Key::KeyS),
        32 => Some(rdev::Key::KeyD),
        33 => Some(rdev::Key::KeyF),
        34 => Some(rdev::Key::KeyG),
        35 => Some(rdev::Key::KeyH),
        36 => Some(rdev::Key::KeyJ),
        37 => Some(rdev::Key::KeyK),
        38 => Some(rdev::Key::KeyL),
        39 => Some(rdev::Key::SemiColon),
        40 => Some(rdev::Key::Quote),
        41 => Some(rdev::Key::BackQuote),
        42 => Some(rdev::Key::ShiftLeft),
        43 => Some(rdev::Key::BackSlash),
        44 => Some(rdev::Key::KeyZ),
        45 => Some(rdev::Key::KeyX),
        46 => Some(rdev::Key::KeyC),
        47 => Some(rdev::Key::KeyV),
        48 => Some(rdev::Key::KeyB),
        49 => Some(rdev::Key::KeyN),
        50 => Some(rdev::Key::KeyM),
        51 => Some(rdev::Key::Comma),
        52 => Some(rdev::Key::Dot),
        53 => Some(rdev::Key::Slash),
        54 => Some(rdev::Key::ShiftRight),
        55 => Some(rdev::Key::KpMultiply),
        56 => Some(rdev::Key::Alt),
        57 => Some(rdev::Key::Space),
        58 => Some(rdev::Key::CapsLock),
        59 => Some(rdev::Key::F1),
        60 => Some(rdev::Key::F2),
        61 => Some(rdev::Key::F3),
        62 => Some(rdev::Key::F4),
        63 => Some(rdev::Key::F5),
        64 => Some(rdev::Key::F6),
        65 => Some(rdev::Key::F7),
        66 => Some(rdev::Key::F8),
        67 => Some(rdev::Key::F9),
        68 => Some(rdev::Key::F10),
        69 => Some(rdev::Key::NumLock),
        70 => Some(rdev::Key::ScrollLock),
        71 => Some(rdev::Key::Kp7),
        72 => Some(rdev::Key::Kp8),
        73 => Some(rdev::Key::Kp9),
        74 => Some(rdev::Key::KpMinus),
        75 => Some(rdev::Key::Kp4),
        76 => Some(rdev::Key::Kp5),
        77 => Some(rdev::Key::Kp6),
        78 => Some(rdev::Key::KpPlus),
        79 => Some(rdev::Key::Kp1),
        80 => Some(rdev::Key::Kp2),
        81 => Some(rdev::Key::Kp3),
        82 => Some(rdev::Key::Kp0),
        83 => Some(rdev::Key::KpDelete),
        87 => Some(rdev::Key::F11),
        88 => Some(rdev::Key::F12),
        96 => Some(rdev::Key::KpReturn),
        97 => Some(rdev::Key::ControlRight),
        98 => Some(rdev::Key::KpDivide),
        99 => Some(rdev::Key::PrintScreen),
        100 => Some(rdev::Key::AltGr),
        102 => Some(rdev::Key::Home),
        103 => Some(rdev::Key::UpArrow),
        104 => Some(rdev::Key::PageUp),
        105 => Some(rdev::Key::LeftArrow),
        106 => Some(rdev::Key::RightArrow),
        107 => Some(rdev::Key::End),
        108 => Some(rdev::Key::DownArrow),
        109 => Some(rdev::Key::PageDown),
        110 => Some(rdev::Key::Insert),
        111 => Some(rdev::Key::Delete),
        119 => Some(rdev::Key::Pause),
        125 => Some(rdev::Key::MetaLeft),
        126 => Some(rdev::Key::MetaRight),
        _ => None,
    }
}

#[cfg(target_os = "linux")]
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

#[cfg(target_os = "macos")]
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
    if !is_trusted() {
        return Err("Permission denied: Accessibility permissions are required to record inputs. Please go to System Settings > Privacy & Security > Accessibility and add/enable your Terminal or application.".to_string());
    }

    let events = Arc::new(Mutex::new(Vec::new()));
    let recording = Arc::new(AtomicBool::new(true));

    let events_clone = Arc::clone(&events);
    let recording_clone = Arc::clone(&recording);

    // Try to pre-populate current cursor coords using core-graphics
    let init_pos = {
        use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
        CGEventSource::new(CGEventSourceStateID::CombinedSessionState).ok().and_then(|source| {
            core_graphics::event::CGEvent::new(source).ok().map(|e| {
                let loc = e.location();
                (loc.x, loc.y)
            })
        })
    };
    let last_coords = Arc::new(Mutex::new(init_pos));

    let handle = thread::spawn(move || {
        let start_time = std::time::Instant::now();

        let callback = move |event: rdev::Event| {
            if !recording_clone.load(Ordering::SeqCst) {
                return;
            }

            let mut events_lock = events_clone.lock().unwrap();
            let time_us = start_time.elapsed().as_micros() as u64;

            match event.event_type {
                rdev::EventType::KeyPress(key) => {
                    if !no_keyboard {
                        let code = rdev_key_to_evdev(key);
                        if code != 0 {
                            events_lock.push(RecordedEvent {
                                time_us,
                                event_type: 1,
                                code,
                                value: 1,
                            });
                        }
                    }
                }
                rdev::EventType::KeyRelease(key) => {
                    if !no_keyboard {
                        let code = rdev_key_to_evdev(key);
                        if code != 0 {
                            events_lock.push(RecordedEvent {
                                time_us,
                                event_type: 1,
                                code,
                                value: 0,
                            });
                        }
                    }
                }
                rdev::EventType::ButtonPress(button) => {
                    if !no_mouse {
                        let code = match button {
                            rdev::Button::Left => 272,
                            rdev::Button::Right => 273,
                            rdev::Button::Middle => 274,
                            rdev::Button::Unknown(c) => 272 + c as u16,
                        };
                        events_lock.push(RecordedEvent {
                            time_us,
                            event_type: 1,
                            code,
                            value: 1,
                        });
                    }
                }
                rdev::EventType::ButtonRelease(button) => {
                    if !no_mouse {
                        let code = match button {
                            rdev::Button::Left => 272,
                            rdev::Button::Right => 273,
                            rdev::Button::Middle => 274,
                            rdev::Button::Unknown(c) => 272 + c as u16,
                        };
                        events_lock.push(RecordedEvent {
                            time_us,
                            event_type: 1,
                            code,
                            value: 0,
                        });
                    }
                }
                rdev::EventType::MouseMove { x, y } => {
                    if !no_mouse {
                        let mut last_lock = last_coords.lock().unwrap();
                        if let Some((last_x, last_y)) = *last_lock {
                            let dx = (x - last_x) as i32;
                            let dy = (y - last_y) as i32;
                            *last_lock = Some((x, y));
                            if dx != 0 {
                                events_lock.push(RecordedEvent {
                                    time_us,
                                    event_type: 2,
                                    code: 0, // REL_X
                                    value: dx,
                                });
                            }
                            if dy != 0 {
                                events_lock.push(RecordedEvent {
                                    time_us,
                                    event_type: 2,
                                    code: 1, // REL_Y
                                    value: dy,
                                });
                            }
                            if dx != 0 || dy != 0 {
                                events_lock.push(RecordedEvent {
                                    time_us,
                                    event_type: 0,
                                    code: 0,
                                    value: 0,
                                });
                            }
                        } else {
                            *last_lock = Some((x, y));
                        }
                    }
                }
                rdev::EventType::Wheel { delta_x, delta_y } => {
                    if !no_mouse {
                        if delta_y != 0 {
                            events_lock.push(RecordedEvent {
                                time_us,
                                event_type: 2,
                                code: 8, // REL_WHEEL
                                value: delta_y as i32,
                            });
                        }
                        if delta_x != 0 {
                            events_lock.push(RecordedEvent {
                                time_us,
                                event_type: 2,
                                code: 6, // REL_HWHEEL
                                value: delta_x as i32,
                            });
                        }
                        if delta_x != 0 || delta_y != 0 {
                            events_lock.push(RecordedEvent {
                                time_us,
                                event_type: 0,
                                code: 0,
                                value: 0,
                            });
                        }
                    }
                }
            }
        };

        if let Err(e) = rdev::listen(callback) {
            eprintln!("Error listening for events: {:?}", e);
        }
    });

    Ok((events, recording, vec![handle]))
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
                if cfg!(target_os = "macos") {
                    eprintln!("Accessibility permissions are required to record inputs.");
                    eprintln!("Please go to System Settings > Privacy & Security > Accessibility");
                    eprintln!("and ensure your Terminal (e.g. Terminal, iTerm2, Alacritty) is allowed.");
                } else {
                    eprintln!("Cannot access input devices in /dev/input/.");
                    eprintln!("Please run this tool with sudo, or add your user to the 'input' group:");
                    eprintln!("    sudo usermod -aG input $USER");
                    eprintln!(
                        "(Note: You will need to log out and log back in for group changes to take effect.)"
                    );
                }
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
                "Active Devices/Hooks",
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
        // Under macOS, the listen thread blocks on native CFRunLoop and won't exit cleanly,
        // but since this is CLI macro recording, the process is about to exit anyway.
        if cfg!(target_os = "linux") {
            h.join().ok();
        }
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
