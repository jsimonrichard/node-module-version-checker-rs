use clap::Parser;
use color_eyre::eyre::{Result, eyre};
use glob::glob;
use log::{info, trace, warn};
use semver::{Version, VersionReq};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = ".")]
    dir: String,
}

/// Reads the package name and version from an installed package's package.json in the given folder
fn get_package_details(folder: &Path) -> Option<(String, Version)> {
    let dep_package_json = folder.join("package.json");
    if dep_package_json.exists() {
        let content = fs::read_to_string(&dep_package_json).ok()?;
        let dep_json: serde_json::Value = serde_json::from_str(&content).ok()?;
        let name = dep_json.get("name").and_then(|n| n.as_str())?.to_string();
        let version_str = dep_json.get("version").and_then(|v| v.as_str())?;
        let version = Version::parse(version_str).ok()?;
        Some((name, version))
    } else {
        None
    }
}

/// Recursively collects all packages (including scoped) from node_modules
fn collect_node_modules_deps(node_modules_path: &Path) -> HashMap<String, Version> {
    let mut node_modules_deps = HashMap::new();
    let entries = match fs::read_dir(node_modules_path) {
        Ok(e) => e,
        Err(_) => return node_modules_deps,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let dir_name = path.file_name().unwrap().to_string_lossy();
            if dir_name.starts_with('@') {
                // Handle scoped packages
                let scoped_entries = match fs::read_dir(&path) {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                for scoped_entry in scoped_entries.flatten() {
                    let scoped_path = scoped_entry.path();
                    if scoped_path.is_dir() {
                        if let Some((name, version)) = get_package_details(&scoped_path) {
                            node_modules_deps.insert(name, version);
                        }
                    }
                }
            } else {
                // Handle regular packages
                if let Some((name, version)) = get_package_details(&path) {
                    node_modules_deps.insert(name, version);
                }
            }
        }
    }
    node_modules_deps
}

/// Merges multiple node_modules dependency maps, prioritizing earlier maps
fn merge_node_modules_deps(deps_maps: &[&HashMap<String, Version>]) -> HashMap<String, Version> {
    let mut merged = HashMap::new();
    for deps in deps_maps {
        for (name, version) in deps.to_owned() {
            if !merged.contains_key(name) {
                merged.insert(name.clone(), version.clone());
            }
        }
    }
    merged
}

/// Checks a package.json against a set of node_modules folders
fn check_package_json(
    package_json: &serde_json::Value,
    installed_deps: &HashMap<String, Version>,
) -> Result<bool> {
    info!(
        "Checking package: {}",
        package_json
            .get("name")
            .expect("package has no name")
            .as_str()
            .expect("package name is not a string")
    );

    let mut package_deps = HashMap::new();
    if let Some(dependencies) = package_json.get("dependencies") {
        if let Some(map) = dependencies.as_object() {
            for (name, version) in map {
                if let Some(version_str) = version.as_str() {
                    if let Ok(version_req) = VersionReq::parse(version_str) {
                        package_deps.insert(name.clone(), version_req);
                    }
                }
            }
        }
    }
    if let Some(dev_dependencies) = package_json.get("devDependencies") {
        if let Some(map) = dev_dependencies.as_object() {
            for (name, version) in map {
                if let Some(version_str) = version.as_str() {
                    if let Ok(version_req) = VersionReq::parse(version_str) {
                        package_deps.insert(name.clone(), version_req);
                    }
                }
            }
        }
    }

    if package_deps.is_empty() {
        info!("No dependencies found in package.json");
        return Ok(true);
    }

    let mut all_match = true;
    for (dep, expected_version_req) in &package_deps {
        match installed_deps.get(dep) {
            Some(actual_version) => {
                if !expected_version_req.matches(actual_version) {
                    warn!(
                        "{}: version mismatch (package.json: {}, node_modules: {})",
                        dep, expected_version_req, actual_version
                    );
                    all_match = false;
                } else {
                    trace!(
                        "{}: version matches (package.json: {}, node_modules: {})",
                        dep, expected_version_req, actual_version
                    );
                }
            }
            None => {
                warn!("{}: Not installed in node_modules", dep);
                all_match = false;
            }
        }
    }
    Ok(all_match)
}

fn parse_package_json(package_json_path: &Path) -> Result<serde_json::Value> {
    if !package_json_path.exists() {
        return Err(eyre!(
            "package.json not found at {}",
            package_json_path.display()
        ));
    }
    let package_json_content = fs::read_to_string(package_json_path)?;
    let package_json: serde_json::Value = serde_json::from_str(&package_json_content)?;
    Ok(package_json)
}

fn get_workspace_members(workspaces: &serde_json::Value) -> Result<Vec<PathBuf>> {
    let mut workspace_paths = Vec::new();
    for workspace in workspaces
        .as_array()
        .ok_or(eyre!("workspaces is not an array"))?
    {
        let workspace_slug = workspace
            .as_str()
            .ok_or(eyre!("workspace entry is not a string"))?;
        let paths = glob(workspace_slug)?;
        for path in paths {
            let path = path?;
            if path.join("package.json").exists() {
                workspace_paths.push(path);
            }
        }
    }
    Ok(workspace_paths)
}

fn check_workspace_member(
    workspace_path: PathBuf,
    hoisted_deps: &HashMap<String, Version>,
) -> Result<bool> {
    let workspace_package_json = workspace_path.join("package.json");
    let workspace_json = parse_package_json(&workspace_package_json)?;
    let non_hoisted_deps = collect_node_modules_deps(&workspace_path.join("node_modules"));
    let deps = merge_node_modules_deps(&[&non_hoisted_deps, hoisted_deps]);
    check_package_json(&workspace_json, &deps)
}

fn main() -> Result<()> {
    color_eyre::install()?;
    env_logger::init();

    let args = Args::parse();
    let base_dir = std::env::current_dir()
        .expect("Failed to get current directory")
        .join(&args.dir);
    let package_json_path = base_dir.join("package.json");
    let node_modules_path = base_dir.join("node_modules");

    let package_json = parse_package_json(&package_json_path)?;

    if !node_modules_path.exists() {
        return Err(eyre!(
            "node_modules directory not found in directory: {}",
            base_dir.display()
        ));
    }
    let node_modules_deps = collect_node_modules_deps(&node_modules_path);

    let mut all_match = check_package_json(&package_json, &node_modules_deps)?;

    // Check workspaces if present
    if let Some(workspaces) = package_json.get("workspaces") {
        let workspace_paths = get_workspace_members(workspaces)?;
        for workspace_path in workspace_paths {
            if !check_workspace_member(workspace_path, &node_modules_deps)? {
                all_match = false;
            }
        }
    }

    if all_match {
        info!("All dependencies match the versions specified in package.json");
    }
    Ok(())
}
