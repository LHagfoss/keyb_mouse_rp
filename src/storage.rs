use colored::Colorize;
use std::path::PathBuf;

pub fn get_macro_dir() -> PathBuf {
    // If run under sudo, SUDO_USER contains the original invoking user
    let user = std::env::var("SUDO_USER")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "lucas".to_string());

    let path = if user == "root" {
        PathBuf::from("/root/.local/share/kmrp")
    } else {
        PathBuf::from(format!("/home/{}/.local/share/kmrp", user))
    };

    // Ensure the directory exists
    if !path.exists() {
        std::fs::create_dir_all(&path).ok();
    }

    path
}

pub fn get_macro_path(name: &str) -> PathBuf {
    let mut filename = name.to_string();
    if !filename.ends_with(".json") {
        filename.push_str(".json");
    }

    // 1. Check default storage location
    let mut default_path = get_macro_dir();
    default_path.push(&filename);
    if default_path.exists() {
        return default_path;
    }

    // 2. Check current working directory or absolute/relative paths directly
    let path = PathBuf::from(name);
    if path.exists() {
        return path;
    }

    let mut path_json = path.clone();
    path_json.set_extension("json");
    if path_json.exists() {
        return path_json;
    }

    // Fallback: return path in default storage directory
    default_path
}

pub fn get_latest_macro() -> Option<String> {
    let dir = get_macro_dir();
    let entries = std::fs::read_dir(dir).ok()?;
    let mut files = Vec::new();

    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                if let Ok(metadata) = entry.metadata() {
                    if let Ok(modified) = metadata.modified() {
                        files.push((path, modified));
                    }
                }
            }
        }
    }

    // Sort by modified time descending
    files.sort_by(|a, b| b.1.cmp(&a.1));

    files
        .first()
        .map(|f| f.0.file_name().unwrap().to_string_lossy().into_owned())
}

pub fn list_macros() {
    let dir = get_macro_dir();
    println!(
        "  [{}] Saved macros in {}:",
        "INFO".blue().bold(),
        dir.to_string_lossy().yellow().bold()
    );

    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => {
            println!("  No macros found (directory does not exist).");
            return;
        }
    };

    let mut files = Vec::new();
    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                if let Ok(metadata) = entry.metadata() {
                    let modified = metadata.modified().ok();
                    let size = metadata.len();
                    files.push((path, modified, size));
                }
            }
        }
    }

    if files.is_empty() {
        println!("  No macros found.");
        return;
    }

    // Sort by modified time descending
    files.sort_by(|a, b| b.1.cmp(&a.1));

    println!();
    println!(
        "  {:<35} {:<25} {:<10}",
        "Macro Name".cyan().bold(),
        "Last Modified".cyan().bold(),
        "Size".cyan().bold()
    );
    println!("  {}", "─".repeat(74).cyan());

    for (path, modified, size) in files {
        let name = path.file_stem().unwrap().to_string_lossy();
        let date_str = if let Some(m) = modified {
            let datetime: chrono::DateTime<chrono::Local> = m.into();
            datetime.format("%Y-%m-%d %H:%M:%S").to_string()
        } else {
            "Unknown".to_string()
        };

        let size_str = format!("{:.2} KB", size as f64 / 1024.0);
        println!(
            "  {:<35} {:<25} {:<10}",
            name.yellow().bold(),
            date_str,
            size_str
        );
    }
}
