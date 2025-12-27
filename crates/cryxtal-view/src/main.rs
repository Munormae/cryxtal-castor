use anyhow::Result;
use clap::Parser;

mod cli;
mod elements;
mod headless;
#[cfg(feature = "gui")]
mod gui;
#[cfg(feature = "gui")]
mod viewer;

fn main() -> Result<()> {
    let args = cli::CliArgs::parse();
    match args.mode {
        Some(cli::Mode::Headless { command }) => headless::run_headless(command),
        None => run_gui(),
    }
}

#[cfg(feature = "gui")]
fn run_gui() -> Result<()> {
    gui::run_gui()
}

#[cfg(not(feature = "gui"))]
fn run_gui() -> Result<()> {
    anyhow::bail!("GUI support disabled. Rebuild with --features gui.");
}
