use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use cryxtal_base::Guid;
use cryxtal_bim::{BimCategory, BimElement, ParameterSet, ParameterValue};
use cryxtal_io::{DEFAULT_TESSELLATION_TOLERANCE, export_obj, export_step};
use cryxtal_shapeops::{DEFAULT_SHAPEOPS_TOLERANCE, plate_with_hole};
use cryxtal_topology::SolidBuilder;
use std::path::PathBuf;
use tracing::info;

#[derive(Parser)]
#[command(name = "cryxtal")]
#[command(about = "CryXtal Castor BIM kernel CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Generate {
        #[command(subcommand)]
        command: GenerateCommand,
    },
    Triangulate(TriangulateArgs),
}

#[derive(Subcommand)]
enum GenerateCommand {
    Box(BoxArgs),
    Plate(PlateArgs),
}

#[derive(Args)]
struct BoxArgs {
    #[arg(long)]
    size: String,
    #[arg(long)]
    out: PathBuf,
    #[arg(long)]
    name: Option<String>,
}

#[derive(Args)]
struct PlateArgs {
    #[arg(long)]
    width: f64,
    #[arg(long)]
    height: f64,
    #[arg(long)]
    thickness: f64,
    #[arg(long)]
    hole: f64,
    #[arg(long)]
    material: Option<String>,
    #[arg(long)]
    out: PathBuf,
    #[arg(long)]
    name: Option<String>,
}

#[derive(Args)]
struct TriangulateArgs {
    #[arg(long = "in")]
    input: PathBuf,
    #[arg(long)]
    out: PathBuf,
}

fn main() -> Result<()> {
    init_tracing();
    let cli = Cli::parse();

    match cli.command {
        Command::Generate {
            command: GenerateCommand::Box(args),
        } => generate_box(args),
        Command::Generate {
            command: GenerateCommand::Plate(args),
        } => generate_plate(args),
        Command::Triangulate(args) => triangulate(args),
    }
}

fn generate_box(args: BoxArgs) -> Result<()> {
    let (width, height, depth) = parse_size(&args.size)?;
    let solid =
        SolidBuilder::box_solid(width, height, depth).context("failed to build box solid")?;

    let mut parameters = ParameterSet::new();
    parameters.insert("Width".to_string(), ParameterValue::Number(width));
    parameters.insert("Height".to_string(), ParameterValue::Number(height));
    parameters.insert("Depth".to_string(), ParameterValue::Number(depth));

    let name = args.name.unwrap_or_else(|| "Box".to_string());
    let element = BimElement::new(Guid::new(), name, BimCategory::Generic, parameters, solid);

    export_step(element.geometry(), &args.out).context("STEP export failed")?;
    info!(path = %args.out.display(), "STEP export complete");
    Ok(())
}

fn generate_plate(args: PlateArgs) -> Result<()> {
    let solid = plate_with_hole(
        args.width,
        args.height,
        args.thickness,
        args.hole,
        DEFAULT_SHAPEOPS_TOLERANCE,
    )
    .context("failed to build plate with hole")?;

    let mut parameters = ParameterSet::new();
    parameters.insert("Width".to_string(), ParameterValue::Number(args.width));
    parameters.insert("Height".to_string(), ParameterValue::Number(args.height));
    parameters.insert(
        "Thickness".to_string(),
        ParameterValue::Number(args.thickness),
    );
    parameters.insert(
        "HoleDiameter".to_string(),
        ParameterValue::Number(args.hole),
    );
    if let Some(material) = args.material {
        parameters.insert("Material".to_string(), ParameterValue::Text(material));
    }

    let name = args.name.unwrap_or_else(|| "PlateWithHole".to_string());
    let element = BimElement::new(Guid::new(), name, BimCategory::Slab, parameters, solid);

    export_obj(
        element.geometry(),
        &args.out,
        DEFAULT_TESSELLATION_TOLERANCE,
    )
    .context("OBJ export failed")?;
    info!(path = %args.out.display(), "OBJ export complete");
    Ok(())
}

fn triangulate(args: TriangulateArgs) -> Result<()> {
    let _ = args.out;
    bail!(
        "STEP import is not implemented yet (requested input: {})",
        args.input.display()
    );
}

fn parse_size(text: &str) -> Result<(f64, f64, f64)> {
    let parts: Vec<&str> = text.split(',').collect();
    if parts.len() != 3 {
        bail!("--size expects three comma-separated numbers, e.g. 100,200,300");
    }

    let width: f64 = parts[0].trim().parse().context("invalid width")?;
    let height: f64 = parts[1].trim().parse().context("invalid height")?;
    let depth: f64 = parts[2].trim().parse().context("invalid depth")?;
    Ok((width, height, depth))
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
