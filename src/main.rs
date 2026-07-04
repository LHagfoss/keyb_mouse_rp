mod args;
mod playback;
mod record;
mod storage;
mod tui;
mod ui;

use args::{Cli, Commands};
use clap::Parser;

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Record { name, no_mouse, no_keyboard }) => {
            ui::print_logo();
            record::record_macro(name, no_mouse, no_keyboard);
        }
        Some(Commands::Play { name, delay, speed, no_mouse, no_keyboard }) => {
            ui::print_logo();
            playback::play_macro(name, delay, speed, no_mouse, no_keyboard);
        }
        Some(Commands::List) => {
            ui::print_logo();
            storage::list_macros();
        }
        None => {
            if let Err(e) = tui::run_tui() {
                eprintln!("TUI Error: {}", e);
            }
        }
    }
}
