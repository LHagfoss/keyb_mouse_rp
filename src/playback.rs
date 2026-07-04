use evdev::{
    AttributeSet, Device, EventType, InputEvent, KeyCode, RelativeAxisCode, uinput::VirtualDevice,
};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering, AtomicUsize};
use std::thread;
use std::time::Duration;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RecordedEvent {
    pub time_us: u64,
    pub event_type: u16,
    pub code: u16,
    pub value: i32,
}

pub fn start_background_playback(
    name_str: String,
    delay_ms: i64,
    speed: f64,
    no_mouse: bool,
    no_keyboard: bool
) -> Result<(
    Arc<AtomicBool>, // playing flag
    Arc<AtomicUsize>, // current event index
    Arc<AtomicBool>, // abort flag
    usize, // total events
    thread::JoinHandle<()>
), String> {
    let path = crate::storage::get_macro_path(&name_str);
    let mut file = File::open(&path).map_err(|_| format!("File {:?} not found.", path))?;
    let mut contents = String::new();
    file.read_to_string(&mut contents).map_err(|e| format!("Failed to read file: {}", e))?;

    let mut events: Vec<RecordedEvent> = serde_json::from_str(&contents)
        .map_err(|e| format!("Failed to parse macro file: {}", e))?;

    if events.is_empty() {
        return Err("Macro contains no events.".to_string());
    }

    events.sort_by_key(|e| e.time_us);

    let mut filtered_events = Vec::new();
    for event in events {
        let is_mouse = event.event_type == 2
            || (event.event_type == 1 && event.code >= 272 && event.code <= 287);
        let is_keyboard = event.event_type == 1 && !is_mouse;

        if is_mouse && no_mouse {
            continue;
        }
        if is_keyboard && no_keyboard {
            continue;
        }
        if event.event_type == 0 && no_mouse && no_keyboard {
            continue;
        }

        filtered_events.push(event);
    }

    let events = filtered_events;

    if events.is_empty() {
        return Err("No events left to play back after filtering.".to_string());
    }

    let total_events = events.len();

    let mut keys = AttributeSet::<KeyCode>::new();
    let mut rel_axes = AttributeSet::<RelativeAxisCode>::new();

    keys.insert(KeyCode::BTN_LEFT);
    keys.insert(KeyCode::BTN_RIGHT);
    keys.insert(KeyCode::BTN_MIDDLE);
    rel_axes.insert(RelativeAxisCode::REL_X);
    rel_axes.insert(RelativeAxisCode::REL_Y);
    rel_axes.insert(RelativeAxisCode::REL_WHEEL);
    rel_axes.insert(RelativeAxisCode::REL_HWHEEL);

    for event in &events {
        if event.event_type == 1 {
            keys.insert(KeyCode(event.code));
        } else if event.event_type == 2 {
            rel_axes.insert(RelativeAxisCode(event.code));
        }
    }

    let mut builder = match VirtualDevice::builder() {
        Ok(b) => b.name("Virtual Macro Device"),
        Err(e) => return Err(format!("Cannot create VirtualDevice builder: {:?}", e)),
    };

    if keys.iter().next().is_some() {
        builder = match builder.with_keys(&keys) {
            Ok(b) => b,
            Err(e) => return Err(format!("Configuring virtual keys: {:?}", e)),
        };
    }

    if rel_axes.iter().next().is_some() {
        builder = match builder.with_relative_axes(&rel_axes) {
            Ok(b) => b,
            Err(e) => return Err(format!("Configuring virtual relative axes: {:?}", e)),
        };
    }

    let mut virtual_device = match builder.build() {
        Ok(d) => d,
        Err(e) => return Err(format!("Building VirtualDevice: {:?}", e)),
    };

    let playing_flag = Arc::new(AtomicBool::new(true));
    let current_event = Arc::new(AtomicUsize::new(0));
    let abort_flag = Arc::new(AtomicBool::new(false));

    let playing_clone = Arc::clone(&playing_flag);
    let current_clone = Arc::clone(&current_event);
    let abort_clone_keyboard = Arc::clone(&abort_flag);
    let abort_clone_playback = Arc::clone(&abort_flag);

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
            let abort_inner = Arc::clone(&abort_clone_keyboard);
            let running_inner = Arc::clone(&running);
            thread::spawn(move || {
                while running_inner.load(Ordering::SeqCst) {
                    if abort_inner.load(Ordering::SeqCst) {
                        break;
                    }
                    if let Ok(events_iter) = dev.fetch_events() {
                        for ev in events_iter {
                            if ev.event_type().0 == 1 {
                                if ev.value() == 1 && (ev.code() == 1 || ev.code() == 16) {
                                    abort_inner.store(true, Ordering::SeqCst);
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

    let playback_handle = thread::spawn(move || {
        let playback_start = std::time::Instant::now();
        let time_us_start = events[0].time_us;
        let delay_us = delay_ms * 1000;

        for (idx, event) in events.into_iter().enumerate() {
            if abort_clone_playback.load(Ordering::SeqCst) {
                break;
            }

            let base_offset_us = (event.time_us as i64 - time_us_start as i64) as f64 / speed;
            let target_time_us = (base_offset_us + delay_us as f64).max(0.0) as u64;

            loop {
                if abort_clone_playback.load(Ordering::SeqCst) {
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

            if abort_clone_playback.load(Ordering::SeqCst) {
                break;
            }

            let ev = InputEvent::new(event.event_type, event.code, event.value);
            if let Err(e) = virtual_device.emit(&[ev]) {
                eprintln!("Failed to simulate event: {:?}", e);
            }
            
            current_clone.store(idx + 1, Ordering::SeqCst);
        }

        playing_clone.store(false, Ordering::SeqCst);
    });

    Ok((playing_flag, current_event, abort_flag, total_events, playback_handle))
}

pub fn play_macro(name: Option<String>, delay_ms: i64, speed: f64, no_mouse: bool, no_keyboard: bool) {
    use colored::Colorize;
    
    let name_str = match name {
        Some(n) => n,
        None => {
            match crate::storage::get_latest_macro() {
                Some(latest) => {
                    println!("  [{}] Playing most recent macro: {}", "INFO".blue().bold(), latest.yellow().bold());
                    latest
                }
                None => {
                    eprintln!("{} No saved macros found. Run 'kmrp record' first.", "Error:".red().bold());
                    return;
                }
            }
        }
    };

    let path = crate::storage::get_macro_path(&name_str);
    let mut file = match File::open(&path) {
        Ok(f) => f,
        Err(_) => {
            eprintln!("{} File {:?} not found.", "Error:".red().bold(), path);
            return;
        }
    };

    let mut contents = String::new();

    if file.read_to_string(&mut contents).is_err() {
        eprintln!("{} Failed to read file {:?}", "Error:".red().bold(), path);
        return;
    }

    let mut events: Vec<RecordedEvent> = match serde_json::from_str(&contents) {
        Ok(events) => events,
        Err(e) => {
            eprintln!("{} Failed to parse file {:?}: {}", "Error:".red().bold(), path, e);
            return;
        }
    };

    if events.is_empty() {
        println!("{}", "No events to play back.".yellow());
        return;
    }

    events.sort_by_key(|e| e.time_us);

    let mut filtered_events = Vec::new();
    for event in events {
        let is_mouse = event.event_type == 2
            || (event.event_type == 1 && event.code >= 272 && event.code <= 287);
        let is_keyboard = event.event_type == 1 && !is_mouse;

        if is_mouse && no_mouse {
            continue;
        }
        if is_keyboard && no_keyboard {
            continue;
        }
        if event.event_type == 0 && no_mouse && no_keyboard {
            continue;
        }

        filtered_events.push(event);
    }

    let events = filtered_events;

    if events.is_empty() {
        println!("{}", "No events left to play back after filtering.".yellow());
        return;
    }

    let mut keys = AttributeSet::<KeyCode>::new();
    let mut rel_axes = AttributeSet::<RelativeAxisCode>::new();

    keys.insert(KeyCode::BTN_LEFT);
    keys.insert(KeyCode::BTN_RIGHT);
    keys.insert(KeyCode::BTN_MIDDLE);
    rel_axes.insert(RelativeAxisCode::REL_X);
    rel_axes.insert(RelativeAxisCode::REL_Y);
    rel_axes.insert(RelativeAxisCode::REL_WHEEL);
    rel_axes.insert(RelativeAxisCode::REL_HWHEEL);

    for event in &events {
        if event.event_type == 1 {
            keys.insert(KeyCode(event.code));
        } else if event.event_type == 2 {
            rel_axes.insert(RelativeAxisCode(event.code));
        }
    }

    let mut builder = match VirtualDevice::builder() {
        Ok(b) => b.name("Virtual Macro Device"),
        Err(e) => {
            eprintln!(
                "{} Cannot create VirtualDevice builder: {:?}",
                "Error:".red().bold(),
                e
            );
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                eprintln!("Permission denied accessing /dev/uinput.");
                eprintln!("Please run this tool with sudo or configure udev rules.");
            }
            return;
        }
    };

    if keys.iter().next().is_some() {
        builder = match builder.with_keys(&keys) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("{} Configuring virtual keys: {:?}", "Error:".red().bold(), e);
                return;
            }
        };
    }

    if rel_axes.iter().next().is_some() {
        builder = match builder.with_relative_axes(&rel_axes) {
            Ok(b) => b,
            Err(e) => {
                eprintln!(
                    "{} Configuring virtual relative axes: {:?}",
                    "Error:".red().bold(),
                    e
                );
                return;
            }
        };
    }

    let mut virtual_device = match builder.build() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{} Building VirtualDevice: {:?}", "Error:".red().bold(), e);
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                eprintln!("Permission denied accessing /dev/uinput. Please run with sudo.");
            }
            return;
        }
    };

    let aborted = Arc::new(AtomicBool::new(false));
    let aborted_clone = Arc::clone(&aborted);

    thread::spawn(move || {
        let mut devices = Vec::new();
        if let Ok(entries) = std::fs::read_dir("/dev/input") {
            for entry in entries {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    if path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map_or(false, |s| s.starts_with("event"))
                    {
                        if let Ok(device) = Device::open(&path) {
                            let name = device.name().unwrap_or("");
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
                            if ev.event_type().0 == 1 {
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

    crate::ui::print_info_box(
        "MACRO PLAYBACK MODULE",
        &[
            format!("{}: {}", "Macro File", path.to_string_lossy().yellow().bold()),
            format!("{}: {}", "Total Events", events.len().to_string().cyan().bold()),
            format!("{}: {}x", "Speed Scale", speed.to_string().cyan().bold()),
            format!("{}: {}ms", "Delay Shift", delay_ms.to_string().cyan().bold()),
            "".to_string(),
            format!("{}:", "How to Abort".yellow().bold()),
            "  - Press physical ESCAPE or Q globally at any time.".to_string(),
            "".to_string(),
            format!("{}: Initializing virtual output device...", "STATUS".blue().bold()),
        ],
    );

    for i in (1..=3).rev() {
        println!("  [{}] Starting in {}s...", "COUNTDOWN".magenta().bold(), i);
        thread::sleep(Duration::from_secs(1));
    }
    println!();
    println!("  [{}] {}", "STATUS".green().bold(), "Playing macro events...".bright_green());
    thread::sleep(Duration::from_millis(500));

    let playback_start = std::time::Instant::now();
    let time_us_start = events[0].time_us;
    let delay_us = delay_ms * 1000;

    for event in events {
        if aborted.load(Ordering::SeqCst) {
            println!("\n  [{}] Playback aborted by user.", "ABORTED".red().bold());
            break;
        }

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
            println!("\n  [{}] Playback aborted by user.", "ABORTED".red().bold());
            break;
        }

        let ev = InputEvent::new(event.event_type, event.code, event.value);
        if let Err(e) = virtual_device.emit(&[ev]) {
            eprintln!("Failed to simulate event: {:?}", e);
        }
    }

    println!();
    println!("  [{}] Playback finished.", "SUCCESS".green().bold());
}
