use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "kmrp")]
#[command(about = "A simple keyboard and mouse macro recorder/player", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Record a keyboard and mouse macro
    Record {
        /// Custom name for the macro (if omitted, a unique timestamp is used)
        name: Option<String>,

        /// Do not record mouse movements and clicks
        #[arg(long)]
        no_mouse: bool,

        /// Do not record keyboard keys
        #[arg(long)]
        no_keyboard: bool,
    },
    /// Play back a saved macro
    Play {
        /// Name of the macro to play (if omitted, plays the most recently recorded macro)
        name: Option<String>,

        /// Offset the replay timeline by this many milliseconds (can be negative to speed up initial lag)
        #[arg(long, short, default_value_t = 0, allow_negative_numbers = true)]
        delay: i64,

        /// Adjust the playback speed multiplier (e.g., 1.0005 to speed up, 0.9995 to slow down drift, 1.5 for 1.5x speed)
        #[arg(long, short, default_value_t = 1.0)]
        speed: f64,

        /// Do not replay mouse movements and clicks
        #[arg(long)]
        no_mouse: bool,

        /// Do not replay keyboard keys
        #[arg(long)]
        no_keyboard: bool,
    },
    /// List all saved macros
    List,
}
