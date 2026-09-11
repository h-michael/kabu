mod cli;
mod color;
mod command;
mod config;
mod error;
mod hook;
mod init;
mod interactive;
mod operation;
mod output;
mod prompt;
mod trust;
mod vcs;

use crate::color::ColorScheme;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};

// Flag to indicate if a termination signal was received
static SIGNAL_RECEIVED: AtomicBool = AtomicBool::new(false);

fn setup_signal_handlers() {
    // ctrlc handles SIGINT (Ctrl+C) on all platforms
    // With `termination` feature, also handles SIGTERM and SIGHUP on Unix
    let _ = ctrlc::set_handler(|| {
        SIGNAL_RECEIVED.store(true, Ordering::SeqCst);
    });
}

fn main() -> ExitCode {
    setup_signal_handlers();

    let args = cli::parse();

    let result = match args.command {
        cli::Command::Add(add_args) => {
            let color_choice = if add_args.no_color {
                clap::ColorChoice::Never
            } else {
                add_args.color
            };
            let color_config = color::ColorConfig::new(color_choice);
            command::add(add_args, color_config)
        }
        cli::Command::Setup(setup_args) => {
            let color_choice = if setup_args.no_color {
                clap::ColorChoice::Never
            } else {
                setup_args.color
            };
            let color_config = color::ColorConfig::new(color_choice);
            command::setup(setup_args, color_config)
        }
        cli::Command::Remove(remove_args) => {
            let color_choice = if remove_args.no_color {
                clap::ColorChoice::Never
            } else {
                remove_args.color
            };
            let color_config = color::ColorConfig::new(color_choice);
            command::remove(remove_args, color_config)
        }
        cli::Command::List(list_args) => {
            let color_choice = if list_args.no_color {
                clap::ColorChoice::Never
            } else {
                list_args.color
            };
            let color_config = color::ColorConfig::new(color_choice);
            command::list(list_args, color_config)
        }
        cli::Command::Path(path_args) => {
            color::ColorConfig::new(clap::ColorChoice::Auto);
            command::path(path_args)
        }
        cli::Command::Cd => {
            color::ColorConfig::new(clap::ColorChoice::Auto);
            command::cd()
        }
        cli::Command::Config(config_args) => {
            color::ColorConfig::new(clap::ColorChoice::Auto);
            command::config(config_args.command)
        }
        cli::Command::Trust(trust_args) => {
            let color_choice = if trust_args.no_color {
                clap::ColorChoice::Never
            } else {
                trust_args.color
            };
            let color_config = color::ColorConfig::new(color_choice);
            command::trust(trust_args, color_config)
        }
        cli::Command::Untrust(untrust_args) => {
            color::ColorConfig::new(clap::ColorChoice::Auto);
            command::untrust(untrust_args)
        }
        cli::Command::Completions { shell } => {
            color::ColorConfig::new(clap::ColorChoice::Auto);
            command::completions(shell)
        }
        cli::Command::Init(init_args) => {
            color::ColorConfig::new(clap::ColorChoice::Auto);
            command::init(init_args)
        }
        cli::Command::Man => {
            color::ColorConfig::new(clap::ColorChoice::Auto);
            command::man()
        }
    };

    // Check if a signal was received
    if SIGNAL_RECEIVED.load(Ordering::SeqCst) {
        // Exit with 130 (128 + SIGINT) - standard for Ctrl+C termination
        // This is the most common case across platforms
        return ExitCode::from(130);
    }

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error::Error::Aborted) => {
            // User-initiated cancellation (e.g., Escape key) - not an error
            ExitCode::SUCCESS
        }
        Err(error::Error::HooksNotTrusted) => {
            // Detailed message already displayed by command
            ExitCode::FAILURE
        }
        Err(error::Error::TrustCheckFailed) => ExitCode::from(1),
        Err(error::Error::CdRequiresShellIntegration) => {
            eprintln!(
                "{}",
                ColorScheme::error(&error::Error::CdRequiresShellIntegration.to_string())
            );
            eprintln!("\nFor setup instructions, run: kabu init --help");
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("{}", ColorScheme::error(&e.to_string()));
            ExitCode::FAILURE
        }
    }
}
