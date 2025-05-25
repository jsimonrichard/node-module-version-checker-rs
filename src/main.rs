use clap::Parser;
use color_eyre::eyre::{Result, eyre};
use colored::*;
use package::PackageJsonData;
use ptree::PrintConfig;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{instrument, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use workspace_resolver::WorkspaceResolver;

mod extended_version_req;
mod package;
mod tree_impl;
mod workspace_resolver;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,

    #[arg(short, long)]
    depth: Option<usize>,
}

#[derive(Parser, Debug)]
enum Commands {
    /// Show dependency tree for a package
    Tree { packages: Vec<PathBuf> },
    /// Compare dependencies between two packages
    Diff { first: PathBuf, second: PathBuf },
}

fn install_tracing() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();
}

#[instrument]
fn main() -> Result<()> {
    color_eyre::install()?;
    install_tracing();

    let args = Args::parse();
    let depth = args.depth.unwrap_or(usize::MAX);
    match args.command {
        Commands::Tree { packages } => handle_tree_command(packages, depth),
        Commands::Diff { first, second } => Ok(()), //handle_diff_command(first, second),
    }
}

#[instrument]
fn handle_tree_command(packages: Vec<PathBuf>, depth: usize) -> Result<()> {
    let mut workspace_resolver = WorkspaceResolver::new(depth);
    let config = PrintConfig {
        depth: depth as u32,
        ..Default::default()
    };

    let packages_data = packages
        .iter()
        .map(|p| {
            PackageJsonData::from_folder(p)
                .and_then(|o| o.ok_or(eyre!("No package.json found at {}", p.display())))
        })
        .collect::<Result<Vec<_>>>()?;

    for package_data in packages_data {
        let mut package_resolver = workspace_resolver.get_package_resolver(&package_data)?;
        let root_package = package_resolver.resolve_root_package(package_data.clone())?;

        if package_data.is_workspace_root() {
            println!("{}", "[WORKSPACE ROOT]".blue());
        }

        ptree::print_tree_with(&root_package, &config).expect("Unable to print dependency tree");
        println!("\n");

        for workspace in package_data.get_workspaces()? {
            let mut package_resolver = workspace_resolver.get_package_resolver(&workspace)?;
            let workspace_package = package_resolver.resolve_root_package(workspace)?;
            ptree::print_tree_with(&workspace_package, &config)
                .expect("Unable to print dependency tree");
            println!("\n");
        }
    }

    Ok(())
}
