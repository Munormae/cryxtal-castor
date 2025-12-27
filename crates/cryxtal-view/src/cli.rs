use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "cryxtal-view")]
#[command(about = "CryXtal Castor GUI and headless app")]
pub struct CliArgs {
    #[command(subcommand)]
    pub mode: Option<Mode>,
}

#[derive(Subcommand)]
pub enum Mode {
    Headless {
        #[command(subcommand)]
        command: HeadlessCommand,
    },
}

#[derive(Subcommand)]
pub enum HeadlessCommand {
    Generate {
        #[command(subcommand)]
        command: GenerateCommand,
    },
    Triangulate(TriangulateArgs),
}

#[derive(Subcommand)]
pub enum GenerateCommand {
    Box(BoxArgs),
    Plate(PlateArgs),
}

#[derive(Args)]
pub struct BoxArgs {
    #[arg(long)]
    pub size: String,
    #[arg(long)]
    pub out: String,
    #[arg(long)]
    pub name: Option<String>,
}

#[derive(Args)]
pub struct PlateArgs {
    #[arg(long)]
    pub width: f64,
    #[arg(long)]
    pub height: f64,
    #[arg(long)]
    pub thickness: f64,
    #[arg(long)]
    pub hole: f64,
    #[arg(long)]
    pub material: Option<String>,
    #[arg(long)]
    pub out: String,
    #[arg(long)]
    pub name: Option<String>,
}

#[derive(Args)]
pub struct TriangulateArgs {
    #[arg(long = "in")]
    pub input: String,
    #[arg(long)]
    pub out: String,
}
