mod args;
mod playback;
mod record;

use args::{Cli, Commands};
use clap::Parser;

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Record { no_mouse, no_keyboard } => {
            record::record_macro(no_mouse, no_keyboard);
        }
        Commands::Play { delay, speed, no_mouse, no_keyboard } => {
            playback::play_macro(delay, speed, no_mouse, no_keyboard);
        }
    }
}
