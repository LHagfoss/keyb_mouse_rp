mod args;
mod playback;
mod record;
mod storage;
mod ui;

use args::{Cli, Commands};
use clap::Parser;

fn main() {
    let cli = Cli::parse();
    ui::print_logo();

    match cli.command {
        Commands::Record { name, no_mouse, no_keyboard } => {
            record::record_macro(name, no_mouse, no_keyboard);
        }
        Commands::Play { name, delay, speed, no_mouse, no_keyboard } => {
            playback::play_macro(name, delay, speed, no_mouse, no_keyboard);
        }
        Commands::List => {
            storage::list_macros();
        }
    }
}
