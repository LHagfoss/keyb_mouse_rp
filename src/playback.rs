use evdev::{uinput::VirtualDevice, AttributeSet, KeyCode, RelativeAxisCode, InputEvent, Device, EventType};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RecordedEvent {
    pub time_us: u64,
    pub event_type: u16,
    pub code: u16,
    pub value: i32,
}

pub fn play_macro(delay_ms: i64, speed: f64, no_mouse: bool, no_keyboard: bool) {
    let mut file = match File::open("macro.json") {
        Ok(f) => f,
        Err(_) => {
            eprintln!("Error: macro.json not found. Run 'cargo run -- record' first");
            return;
        }
    };

    let mut contents = String::new();

    if file.read_to_string(&mut contents).is_err() {
        eprintln!("Error: Failed to read macro.json");
        return;
    }

    let mut events: Vec<RecordedEvent> = match serde_json::from_str(&contents) {
        Ok(events) => events,
        Err(e) => {
            eprintln!("Error: Failed to parse macro.json: {}", e);
            return;
        }
    };

    if events.is_empty() {
        println!("No events to play back.");
        return;
    }

    // Ensure they are sorted chronologically
    events.sort_by_key(|e| e.time_us);

    // Apply filtering for mouse and keyboard events
    let mut filtered_events = Vec::new();
    for event in events {
        // Event type 1 is EV_KEY. Button codes in mouse range (272 to 287 or BTN_MOUSE = 0x110)
        let is_mouse = event.event_type == 2 || (event.event_type == 1 && event.code >= 272 && event.code <= 287);
        let is_keyboard = event.event_type == 1 && !is_mouse;

        if is_mouse && no_mouse {
            continue;
        }
        if is_keyboard && no_keyboard {
            continue;
        }
        // Always allow EV_SYN (0) or other types, unless we are ignoring both
        if event.event_type == 0 && no_mouse && no_keyboard {
            continue;
        }

        filtered_events.push(event);
    }

    let events = filtered_events;

    if events.is_empty() {
        println!("No events left to play back after applying filters.");
        return;
    }

    let mut keys = AttributeSet::<KeyCode>::new();
    let mut rel_axes = AttributeSet::<RelativeAxisCode>::new();

    // Always declare standard mouse capabilities so that libinput/OS
    // correctly categorizes the virtual uinput device as a mouse pointer.
    keys.insert(KeyCode::BTN_LEFT);
    keys.insert(KeyCode::BTN_RIGHT);
    keys.insert(KeyCode::BTN_MIDDLE);
    rel_axes.insert(RelativeAxisCode::REL_X);
    rel_axes.insert(RelativeAxisCode::REL_Y);
    rel_axes.insert(RelativeAxisCode::REL_WHEEL);
    rel_axes.insert(RelativeAxisCode::REL_HWHEEL);

    for event in &events {
        if event.event_type == 1 { // EV_KEY
            keys.insert(KeyCode(event.code));
        } else if event.event_type == 2 { // EV_REL
            rel_axes.insert(RelativeAxisCode(event.code));
        }
    }

    let mut builder = match VirtualDevice::builder() {
        Ok(b) => b.name("Virtual Macro Device"),
        Err(e) => {
            eprintln!("Error: Cannot create VirtualDevice builder: {:?}", e);
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                eprintln!("Permission denied accessing /dev/uinput.");
                eprintln!("Please run this tool with sudo.");
            }
            return;
        }
    };

    if keys.iter().next().is_some() {
        builder = match builder.with_keys(&keys) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("Error configuring virtual keys: {:?}", e);
                return;
            }
        };
    }

    if rel_axes.iter().next().is_some() {
        builder = match builder.with_relative_axes(&rel_axes) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("Error configuring virtual relative axes: {:?}", e);
                return;
            }
        };
    }

    let mut virtual_device = match builder.build() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Error building VirtualDevice: {:?}", e);
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                eprintln!("Permission denied accessing /dev/uinput. Please run with sudo.");
            }
            return;
        }
    };

    let aborted = Arc::new(AtomicBool::new(false));
    let aborted_clone = Arc::clone(&aborted);

    // Spawn a thread to listen to physical keyboards for the panic button (ESC or Q)
    thread::spawn(move || {
        let mut devices = Vec::new();
        if let Ok(entries) = std::fs::read_dir("/dev/input") {
            for entry in entries {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    if path.file_name().and_then(|n| n.to_str()).map_or(false, |s| s.starts_with("event")) {
                        if let Ok(device) = Device::open(&path) {
                            let name = device.name().unwrap_or("");
                            // Skip our own virtual device to prevent self-triggering
                            if name == "Virtual Macro Device" {
                                continue;
                            }
                            let supports_keys = device.supported_events().contains(EventType::KEY);
                            if supports_keys {
                                devices.push(device);
                            }
                        }
                    }
                }
            }
        }

        let running = Arc::new(AtomicBool::new(true));
        for mut dev in devices {
            let aborted_inner = Arc::clone(&aborted_clone);
            let running_inner = Arc::clone(&running);
            thread::spawn(move || {
                while running_inner.load(Ordering::SeqCst) {
                    if aborted_inner.load(Ordering::SeqCst) {
                        break;
                    }
                    if let Ok(events_iter) = dev.fetch_events() {
                        for ev in events_iter {
                            if ev.event_type().0 == 1 { // EV_KEY
                                // Code 1 is KEY_ESC, Code 16 is KEY_Q
                                if ev.value() == 1 && (ev.code() == 1 || ev.code() == 16) {
                                    aborted_inner.store(true, Ordering::SeqCst);
                                    break;
                                }
                            }
                        }
                    } else {
                        break;
                    }
                }
            });
        }
    });

    println!("Playing back {} events in 3 seconds...", events.len());
    println!("Press physical ESCAPE or Q globally at any time to abort playback.");
    thread::sleep(Duration::from_secs(3));

    let playback_start = std::time::Instant::now();
    let time_us_start = events[0].time_us;
    let delay_us = delay_ms * 1000;

    for event in events {
        if aborted.load(Ordering::SeqCst) {
            println!("\nPlayback aborted by user.");
            break;
        }

        // Calculate raw target offset in microseconds, apply speed multiplier, then shift by delay_us
        let base_offset_us = (event.time_us as i64 - time_us_start as i64) as f64 / speed;
        let target_time_us = (base_offset_us + delay_us as f64).max(0.0) as u64;

        loop {
            if aborted.load(Ordering::SeqCst) {
                break;
            }
            let elapsed_us = playback_start.elapsed().as_micros() as u64;
            if elapsed_us >= target_time_us {
                break;
            }
            let remaining_us = target_time_us - elapsed_us;
            if remaining_us > 3000 {
                thread::sleep(Duration::from_micros(remaining_us - 1000));
            } else {
                std::hint::spin_loop();
            }
        }

        if aborted.load(Ordering::SeqCst) {
            println!("\nPlayback aborted by user.");
            break;
        }

        let ev = InputEvent::new(event.event_type, event.code, event.value);
        if let Err(e) = virtual_device.emit(&[ev]) {
            eprintln!("Failed to simulate event: {:?}", e);
        }
    }

    println!("\nPlayback finished.");
}
