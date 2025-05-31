use clap::Parser;
use color_eyre::eyre::{Result, eyre};
use colored::*;
use diff::Differ;
use ptree::{PrintConfig, Style as PStyle};
use resolver::Resolver;
use std::path::PathBuf;
use tracing::debug;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod dependency_resolver;
mod diff;
mod extended_version_req;
mod node_modules;
mod package;
mod package_data;
mod ptree_impl;
mod resolver;
mod workspace_data;

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
    /// Show the dependency tree for a package
    Tree { packages: Vec<PathBuf> },
    /// Compare dependencies between two packages
    Diff { left: PathBuf, right: PathBuf },
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
        Commands::Diff {
            left: first,
            right: second,
        } => handle_diff_command(first, second, config),
    }
}

fn handle_tree_command(packages: Vec<PathBuf>, config: PrintConfig) -> Result<()> {
    let mut resolver = Resolver::new(config.depth as usize);

    for package_path in packages {
        let package = resolver.resolve(&package_path)?;

        debug!("Printing dependency tree for: {}", package.name);
        if package.data.is_workspace_root() {
            println!("{}", "[WORKSPACE ROOT]".blue());
        }

        package
            .print_tree(&config)
            .expect("Unable to print dependency tree");
        println!("");

        if let Some(workspace_data) = package.data.workspace_data.clone() {
            for workspace_package in
                resolver.resolve_workspace_members(&package_path, &workspace_data)?
            {
                println!("{}", "[WORKSPACE MEMBER]".blue());
                workspace_package
                    .print_tree(&config)
                    .expect("Unable to print dependency tree");
                println!("");
            }
        }
    }

    Ok(())
}

fn handle_diff_command(left: PathBuf, right: PathBuf, config: PrintConfig) -> Result<()> {
    // let mut workspace_resolver = WorkspaceResolver::new(config.depth as usize);
    let mut resolver = Resolver::new(config.depth as usize);

    let left_package = resolver.resolve(&left)?;
    let right_package = resolver.resolve(&right)?;

    let (_differ, diff) = Differ::diff(left_package.clone(), right_package.clone());

    let diff = diff.ok_or(eyre!("Unable to diff packages"))?;

    diff.print_tree(&config)
        .expect("Unable to print dependency tree");

    Ok(())
}
