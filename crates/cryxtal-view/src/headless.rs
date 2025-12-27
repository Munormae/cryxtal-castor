use anyhow::{Context, Result, bail};
use cryxtal_io::{DEFAULT_TESSELLATION_TOLERANCE, export_obj, export_step};

use crate::cli::{GenerateCommand, HeadlessCommand};
use crate::elements::{build_box_element, build_plate_element};

pub fn run_headless(command: HeadlessCommand) -> Result<()> {
    match command {
        HeadlessCommand::Generate {
            command: GenerateCommand::Box(args),
        } => {
            let (width, height, depth) = parse_size(&args.size)?;
            let element = build_box_element(width, height, depth, args.name.as_deref())?;
            export_step(element.geometry(), &args.out)?;
            println!("STEP exported: {}", args.out);
            Ok(())
        }
        HeadlessCommand::Generate {
            command: GenerateCommand::Plate(args),
        } => {
            let element = build_plate_element(
                args.width,
                args.height,
                args.thickness,
                args.hole,
                args.material.as_deref(),
                args.name.as_deref(),
            )?;
            export_obj(
                element.geometry(),
                &args.out,
                DEFAULT_TESSELLATION_TOLERANCE,
            )?;
            println!("OBJ exported: {}", args.out);
            Ok(())
        }
        HeadlessCommand::Triangulate(args) => {
            let _ = args.out;
            bail!(
                "STEP import is not implemented yet (requested input: {})",
                args.input
            )
        }
    }
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
