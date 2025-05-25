use clap::Parser;
use color_eyre::eyre::{Result, eyre};
use colored::*;
use diff::DiffedPackage;
use package::PackageJsonData;
use ptree::{PrintConfig, Style as PStyle};
use std::path::PathBuf;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use workspace_resolver::WorkspaceResolver;

mod diff;
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

fn main() -> Result<()> {
    color_eyre::install()?;
    install_tracing();

    let args = Args::parse();
    let depth = args.depth.unwrap_or(usize::MAX);
    let config = PrintConfig {
        depth: depth as u32,
        branch: PStyle {
            dimmed: true,
            ..Default::default()
        },
        ..Default::default()
    };

    match args.command {
        Commands::Tree { packages } => handle_tree_command(packages, config),
        Commands::Diff { first, second } => handle_diff_command(first, second, config),
    }
}

fn handle_tree_command(packages: Vec<PathBuf>, config: PrintConfig) -> Result<()> {
    let mut workspace_resolver = WorkspaceResolver::new(config.depth as usize);

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

fn handle_diff_command(first: PathBuf, second: PathBuf, config: PrintConfig) -> Result<()> {
    let mut workspace_resolver = WorkspaceResolver::new(config.depth as usize);

    let first_package_data = PackageJsonData::from_folder(&first)?
        .ok_or(eyre!("No package.json found at {}", first.display()))?;
    let second_package_data = PackageJsonData::from_folder(&second)?
        .ok_or(eyre!("No package.json found at {}", second.display()))?;

    let mut first_package_resolver =
        workspace_resolver.get_package_resolver(&first_package_data)?;
    let mut second_package_resolver =
        workspace_resolver.get_package_resolver(&second_package_data)?;

    let first_package = first_package_resolver.resolve_root_package(first_package_data)?;
    let second_package = second_package_resolver.resolve_root_package(second_package_data)?;

    info!("Diffing packages");
    let diff = DiffedPackage::from(first_package, second_package)
        .ok_or(eyre!("Unable to diff packages"))?;

    info!("Printing dependency tree");
    ptree::print_tree_with(&diff, &config).expect("Unable to print dependency tree");

    Ok(())
}
