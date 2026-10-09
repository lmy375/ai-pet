//! `axcu` — command-line front end for ax-computer-use.
//!
//! Workflow: `axcu state <app>` prints the element tree with indices, then
//! the action subcommands reference those indices. The walk is deterministic,
//! so indices hold across invocations until the app's UI changes.

use ax_computer_use::{get_app_state, list_apps, AppState, AxError, ScrollDirection};
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(
    name = "axcu",
    about = "macOS computer use over the Accessibility API: list apps, read app state, click, type, press keys, scroll",
    after_help = "Workflow: run `axcu list-apps` to find a target app, `axcu state <app>` to print its element tree with [index] numbers, then use those indices with the action subcommands."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List on-screen apps (frontmost first) usable with the other subcommands
    ListApps,
    /// Print the app's front window as a text tree with element indices
    State {
        /// App name, case-insensitive substring of the window owner (e.g. "safari")
        app: String,
    },
    /// Click an element by its snapshot index
    Click {
        app: String,
        /// Element index from a previous `axcu state` run
        index: usize,
    },
    /// Type text into an element by its snapshot index
    Type {
        app: String,
        index: usize,
        text: String,
    },
    /// Press a key or combo in the app (e.g. "enter", "cmd+c", "ctrl+shift+tab")
    Key {
        app: String,
        key: String,
    },
    /// Scroll an element by its snapshot index
    Scroll {
        app: String,
        index: usize,
        #[clap(value_enum)]
        direction: DirectionArg,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum DirectionArg {
    Up,
    Down,
}

impl From<DirectionArg> for ScrollDirection {
    fn from(d: DirectionArg) -> Self {
        match d {
            DirectionArg::Up => ScrollDirection::Up,
            DirectionArg::Down => ScrollDirection::Down,
        }
    }
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::ListApps => {
            for app in list_apps() {
                println!("{} | {}", app.pid(), app.name());
            }
            Ok(())
        }
        Command::State { app } => {
            let state = state_of(&app);
            state.map(|s| {
                println!(
                    "pid {} | {} | \"{}\"",
                    s.pid(),
                    s.app_name(),
                    s.window_title()
                );
                println!("{}", s.render());
            })
        }
        Command::Click { app, index } => run(&app, |s| s.click(index)),
        Command::Type { app, index, text } => run(&app, |s| s.type_text(index, &text)),
        Command::Key { app, key } => run(&app, |s| s.press_key(&key)),
        Command::Scroll { app, index, direction } => {
            run(&app, |s| s.scroll(index, direction.into()))
        }
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn state_of(app: &str) -> Result<AppState, AxError> {
    get_app_state(app)
}

fn run<F>(app: &str, action: F) -> Result<(), AxError>
where
    F: FnOnce(&AppState) -> Result<String, AxError>,
{
    let state = state_of(app)?;
    println!("{}", action(&state)?);
    Ok(())
}
