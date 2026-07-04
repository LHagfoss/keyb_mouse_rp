use colored::Colorize;

pub fn print_logo() {
    if let Ok(content) = std::fs::read_to_string("LOGO.txt") {
        println!("{}", content.cyan().bold());
    } else {
        // Fallback ASCII Art
        println!(
            "{}",
            "██╗  ██╗███╗   ███╗██████╗ ██████╗ \n\
             ██║ ██╔╝████╗ ████║██╔══██╗██╔══██╗\n\
             █████╔╝ ██╔████╔██║██████╔╝██████╔╝\n\
             ██╔═██╗ ██║╚██╔╝██║██╔══██╗██╔═══╝ \n\
             ██║  ██╗██║ ╚═╝ ██║██║  ██║██║     \n\
             ╚═╝  ╚═╝╚═╝     ╚═╝╚═╝  ╚═╝╚═╝     ".cyan().bold()
        );
    }
    println!();
}

fn visible_length(s: &str) -> usize {
    let mut len = 0;
    let mut in_escape = false;
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\x1b' {
            in_escape = true;
            i += 1;
            continue;
        }
        if in_escape {
            if chars[i] == 'm' {
                in_escape = false;
            }
            i += 1;
            continue;
        }
        len += 1;
        i += 1;
    }
    len
}

pub fn print_info_box(title: &str, lines: &[String]) {
    let width = 64;
    println!("{}", format!("{}{}{}", "┌".cyan().bold(), "─".repeat(width - 2).cyan().bold(), "┐".cyan().bold()));
    
    // Title row centered
    let title_str = format!(" {} ", title);
    let title_len = title_str.len();
    let left_pad = (width - 2 - title_len) / 2;
    let right_pad = width - 2 - title_len - left_pad;
    println!(
        "{}{}{}{}",
        "│".cyan().bold(),
        " ".repeat(left_pad),
        title_str.yellow().bold(),
        " ".repeat(right_pad)
    );
    
    println!("{}", format!("{}{}{}", "├".cyan().bold(), "─".repeat(width - 2).cyan().bold(), "┤".cyan().bold()));
    
    for line in lines {
        let vis_len = visible_length(line);
        let pad = if width - 2 > vis_len {
            width - 2 - vis_len
        } else {
            0
        };
        
        // Split line and padding spaces
        let padding_spaces = if pad > 1 {
            " ".repeat(pad - 1)
        } else {
            "".to_string()
        };
        
        println!(
            "{} {}{} {}",
            "│".cyan().bold(),
            line,
            padding_spaces,
            "│".cyan().bold()
        );
    }
    println!("{}", format!("{}{}{}", "└".cyan().bold(), "─".repeat(width - 2).cyan().bold(), "┘".cyan().bold()));
    println!();
}
