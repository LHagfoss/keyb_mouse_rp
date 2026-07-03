use crate::playback::RecordedEvent;
use crossterm::event::{self, Event as CrossEvent, KeyCode as CrossKeyCode, KeyEvent};
use evdev::{Device, EventType};
use std::fs::File;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, UNIX_EPOCH};

pub fn record_macro(no_mouse: bool, no_keyboard: bool) {
    let events = Arc::new(Mutex::new(Vec::new()));

    let mut devices = Vec::new();
    let entries = match std::fs::read_dir("/dev/input") {
        Ok(e) => e,
        Err(err) => {
            eprintln!("Error reading /dev/input: {}", err);
            return;
        }
    };

    let mut permission_denied = false;

    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.file_name().and_then(|n| n.to_str()).map_or(false, |s| s.starts_with("event")) {
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
            eprintln!("Permission denied: Cannot access input devices in /dev/input/.");
            eprintln!("Please run this tool with sudo, or add your user to the 'input' group:");
            eprintln!("    sudo usermod -aG input $USER");
            eprintln!("(Note: You will need to log out and log back in for group changes to take effect.)");
        } else {
            eprintln!("No compatible keyboard or mouse input devices found in /dev/input/.");
        }
        return;
    }

    println!("Recording will start in 3 seconds...");
    for i in (1..=3).rev() {
        println!("{}...", i);
        thread::sleep(Duration::from_secs(1));
    }

    println!("Recording started from {} input devices...", devices.len());
    println!("FOCUS THIS TERMINAL and press ESCAPE or Q to stop and save.");

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
                            // Event type 1 is EV_KEY. Button codes in mouse range (272 to 287 or BTN_MOUSE = 0x110)
                            let is_mouse = event.event_type().0 == 2 || (event.event_type().0 == 1 && event.code() >= 272 && event.code() <= 287);
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

                            let time_us = event.timestamp().duration_since(UNIX_EPOCH)
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

    // Filter out the trailing keys from the events list before saving
    let mut events_lock = events.lock().unwrap();
    
    // Sort chronologically using kernel hardware timestamps
    events_lock.sort_by_key(|e| e.time_us);

    trim_exit_events(&mut events_lock);

    save_and_exit(&events_lock);
}

fn trim_exit_events(events: &mut Vec<RecordedEvent>) {
    // We want to remove trailing key events for Q (code 16) or ESC (code 1).
    // An EV_KEY event has event_type = 1.
    // An EV_SYN event has event_type = 0.
    while let Some(last) = events.last() {
        if last.event_type == 0 { // EV_SYN
            events.pop();
        } else if last.event_type == 1 && (last.code == 16 || last.code == 1) { // KEY_Q or KEY_ESC
            events.pop();
        } else {
            break;
        }
    }
}

fn save_and_exit(events_lock: &Vec<RecordedEvent>) {
    let json = match serde_json::to_string_pretty(events_lock) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("Error serializing macro: {}", e);
            return;
        }
    };

    let mut file = File::create("macro.json").expect("Unable to create macro.json");
    file.write_all(json.as_bytes())
        .expect("Unable to write data");

    println!(
        "\r\nSaved {} events to macro.json. Exiting cleanly.",
        events_lock.len()
    );
}
