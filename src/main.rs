use clap::Parser;
use color_eyre::eyre::{Result, eyre};
use colored::*;
use ptree::{PrintConfig, Style, TreeItem};
use semver::{Version, VersionReq};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::hash::Hash;
use std::io;
use std::path::{Path, PathBuf};
use tracing::{instrument, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = ".")]
    dir: String,

    #[arg(long, default_value_t = usize::MAX)]
    depth: usize,
}

#[derive(Debug, Clone)]
enum ExtendedVersionReq {
    SemVer(VersionReq),
    Workspace(String),
}

impl fmt::Display for ExtendedVersionReq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SemVer(req) => write!(f, "{}", req),
            Self::Workspace(path) => write!(f, "workspace:{}", path),
        }
    }
}

impl ExtendedVersionReq {
    #[instrument]
    fn parse(version_str: &str) -> Result<Self> {
        if version_str.starts_with("workspace:") {
            Ok(Self::Workspace(version_str[10..].to_string()))
        } else {
            Ok(Self::SemVer(VersionReq::parse(version_str)?))
        }
    }

    #[instrument]
    fn matches(&self, version: &Version) -> bool {
        match self {
            Self::SemVer(version_req) => version_req.matches(version),
            Self::Workspace(_) => true,
        }
    }
}

#[derive(Debug, Clone)]
struct PartialPackage {
    name: String,
    version: Option<Version>,
    install_path: PathBuf,
    dependencies: HashMap<String, ExtendedVersionReq>,
    dev_dependencies: HashMap<String, ExtendedVersionReq>,
    workspaces_globs: Vec<String>,
}

impl PartialPackage {
    #[instrument]
    fn from_folder(folder: &Path) -> Result<Self> {
        let dep_package_json = folder.join("package.json");
        if !dep_package_json.exists() {
            return Err(eyre!(
                "package.json not found at {}",
                dep_package_json.display()
            ));
        }
        let content = fs::read_to_string(&dep_package_json)?;
        let dep_json: serde_json::Value = serde_json::from_str(&content)?;
        let name = dep_json
            .get("name")
            .and_then(|n| n.as_str())
            .ok_or(eyre!("package has no name"))?
            .to_string();
        let version = dep_json
            .get("version")
            .and_then(|v| v.as_str())
            .map(|v| Version::parse(v))
            .transpose()?;
        let dependencies = dep_json
            .get("dependencies")
            .map(deps_from_json)
            .transpose()?
            .unwrap_or_default();

        // Only load devDependencies if the package is not in node_modules
        let dev_dependencies = if folder.to_string_lossy().contains("node_modules") {
            HashMap::new()
        } else {
            dep_json
                .get("devDependencies")
                .map(deps_from_json)
                .transpose()?
                .unwrap_or_default()
        };

        let workspaces_globs = dep_json
            .get("workspaces")
            .map(|w| {
                w.as_array()
                    .ok_or(eyre!("workspaces is not an array"))?
                    .iter()
                    .map(|w| {
                        Ok(w.as_str()
                            .ok_or(eyre!("workspace entry is not a string"))?
                            .to_string())
                    })
                    .collect::<Result<Vec<String>>>()
            })
            .transpose()?
            .unwrap_or_default();
        Ok(Self {
            name,
            version,
            install_path: folder.to_path_buf(),
            dependencies,
            dev_dependencies,
            workspaces_globs,
        })
    }

    #[instrument]
    fn map_from_node_modules(node_modules_path: &Path) -> Result<HashMap<String, Self>> {
        let mut deps = HashMap::new();

        if !node_modules_path.exists() {
            return Ok(deps);
        }

        for entry in fs::read_dir(node_modules_path)?.flatten() {
            let path = entry.path();

            if !path.is_dir() {
                continue;
            }

            let dir_name = path.file_name().unwrap().to_string_lossy();
            if dir_name.starts_with('@') {
                // Handle scoped packages
                let scoped_entries = fs::read_dir(&path)?;
                for scoped_entry in scoped_entries.flatten() {
                    let scoped_path = scoped_entry.path();
                    if !scoped_path.is_dir() || !scoped_path.join("package.json").exists() {
                        continue;
                    }

                    if let Ok(package) = PartialPackage::from_folder(&scoped_path) {
                        deps.insert(package.name.clone(), package);
                    } else {
                        warn!(
                            "Failed to parse package from folder: {}",
                            scoped_path.display()
                        );
                    }
                }
            } else {
                // Handle regular packages
                if !path.join("package.json").exists() {
                    continue;
                }

                if let Ok(package) = PartialPackage::from_folder(&path) {
                    deps.insert(package.name.clone(), package);
                } else {
                    warn!("Failed to parse package from folder: {}", path.display());
                }
            }
        }

        return Ok(deps);
    }
}

#[instrument]
fn deps_from_json(deps: &serde_json::Value) -> Result<HashMap<String, ExtendedVersionReq>> {
    let mut result = HashMap::new();
    let deps_object = deps
        .as_object()
        .ok_or(eyre!("dependencies is not an object"))?;
    for (name, version) in deps_object {
        let version_str = version.as_str().ok_or(eyre!("version is not a string"))?;
        let version_req = ExtendedVersionReq::parse(version_str)?;
        result.insert(name.clone(), version_req);
    }
    Ok(result)
}

#[derive(Debug, Clone)]
enum ResolvedPackage {
    Resolved(Package),
    Deduped(PackageKey),
    Missing(String),
}

impl fmt::Display for ResolvedPackage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self {
            ResolvedPackage::Resolved(package) => {
                write!(f, "{}", package)
            }
            ResolvedPackage::Deduped(key) => {
                write!(f, "{} {}", key, "[DEDUPED]".yellow())
            }
            ResolvedPackage::Missing(name) => {
                write!(f, "{} {}", name, "[MISSING]".red())
            }
        }
    }
}

#[derive(Debug, Clone)]
struct ResolvedPackageWithVersionReq {
    version_req: ExtendedVersionReq,
    package: ResolvedPackage,
}

impl fmt::Display for ResolvedPackageWithVersionReq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (required: {})", self.package, self.version_req)
    }
}

#[derive(Debug, Clone)]
struct Package {
    name: String,
    version: Option<Version>,
    install_path: PathBuf,
    dependencies: HashMap<String, ResolvedPackageWithVersionReq>,
    dev_dependencies: HashMap<String, ResolvedPackageWithVersionReq>,
}

impl fmt::Display for Package {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(version) = &self.version {
            write!(f, "{}@{}", self.name, version)
        } else {
            write!(f, "{}", self.name)
        }
    }
}

impl TreeItem for Package {
    type Child = PackageTreeChild;

    fn write_self<W: io::Write>(&self, f: &mut W, style: &Style) -> io::Result<()> {
        write!(f, "{}", style.paint(self))
    }

    fn children(&self) -> Cow<[Self::Child]> {
        let mut v: Vec<PackageTreeChild> = self
            .dependencies
            .values()
            .map(|r| PackageTreeChild::Package(r.clone()))
            .collect();

        if !self.dev_dependencies.is_empty() {
            v.push(PackageTreeChild::DevDependencySeparator);
            v.extend(
                self.dev_dependencies
                    .values()
                    .map(|r| PackageTreeChild::Package(r.clone())),
            );
        }

        Cow::from(v)
    }
}

#[derive(Debug, Clone)]
enum PackageTreeChild {
    Package(ResolvedPackageWithVersionReq),
    DevDependencySeparator,
}

impl fmt::Display for PackageTreeChild {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PackageTreeChild::Package(package) => write!(f, "{}", package),
            PackageTreeChild::DevDependencySeparator => {
                write!(f, "{}", "[DEV DEPENDENCIES]".blue())
            }
        }
    }
}

impl TreeItem for PackageTreeChild {
    type Child = PackageTreeChild;

    fn write_self<W: io::Write>(&self, f: &mut W, style: &Style) -> io::Result<()> {
        write!(f, "{}", style.paint(self))
    }

    fn children(&self) -> Cow<[Self::Child]> {
        if let PackageTreeChild::Package(ResolvedPackageWithVersionReq {
            package: ResolvedPackage::Resolved(package),
            ..
        }) = self
        {
            Cow::from(package.children().to_vec())
        } else {
            Cow::Borrowed(&[])
        }
    }
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
struct PackageKey {
    name: String,
    version: Option<Version>,
}

impl fmt::Display for PackageKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(version) = &self.version {
            write!(f, "{}@{}", self.name, version)
        } else {
            write!(f, "{}", self.name)
        }
    }
}

#[derive(Debug)]
struct PackageResolver {
    hoisted_partials: HashMap<String, PartialPackage>,
    resolved_packages: HashMap<PackageKey, Package>,
    visiting: Vec<PackageKey>,
}

impl PackageResolver {
    fn new(hoisted_partials: HashMap<String, PartialPackage>) -> Self {
        Self {
            hoisted_partials,
            resolved_packages: HashMap::new(),
            visiting: Vec::new(),
        }
    }

    fn extend(&self, other: HashMap<String, PartialPackage>) -> Self {
        Self {
            hoisted_partials: merge_hashmaps(&[&other, &self.hoisted_partials]),
            resolved_packages: self.resolved_packages.clone(),
            visiting: self.visiting.clone(),
        }
    }

    #[instrument]
    fn resolve_package(&mut self, partial: PartialPackage) -> Result<ResolvedPackage> {
        let PartialPackage {
            name,
            version,
            install_path,
            dependencies,
            dev_dependencies,
            workspaces_globs: _,
        } = partial;

        let key = PackageKey {
            name: name.clone(),
            version: version.clone(),
        };

        if self.visiting.contains(&key) {
            return Ok(ResolvedPackage::Deduped(key));
        }

        if let Some(package) = self.resolved_packages.get(&key) {
            return Ok(ResolvedPackage::Resolved(package.clone()));
        }

        self.visiting.push(key.clone());

        let node_modules_path = install_path.join("node_modules");

        let resolved_dependencies;
        let resolved_dev_dependencies;
        if node_modules_path.exists() {
            let sub_modules = PartialPackage::map_from_node_modules(&node_modules_path)?;
            let resolver = &mut self.extend(sub_modules);
            resolved_dependencies = resolver.resolve_deps(&dependencies)?;
            resolved_dev_dependencies = resolver.resolve_deps(&dev_dependencies)?;
        } else {
            resolved_dependencies = self.resolve_deps(&dependencies)?;
            resolved_dev_dependencies = self.resolve_deps(&dev_dependencies)?;
        }

        let package = Package {
            name,
            version,
            install_path: install_path.clone(),
            dependencies: resolved_dependencies,
            dev_dependencies: resolved_dev_dependencies,
        };
        self.resolved_packages.insert(key, package.clone());

        Ok(ResolvedPackage::Resolved(package))
    }

    #[instrument]
    fn resolve_deps(
        &mut self,
        deps: &HashMap<String, ExtendedVersionReq>,
    ) -> Result<HashMap<String, ResolvedPackageWithVersionReq>> {
        let mut packages = HashMap::new();
        for (name, version_req) in deps {
            if let Some(partial) = self.hoisted_partials.get(name).cloned() {
                let package = self.resolve_package(partial)?;

                packages.insert(
                    name.clone(),
                    ResolvedPackageWithVersionReq {
                        version_req: version_req.clone(),
                        package,
                    },
                );
            } else {
                packages.insert(
                    name.clone(),
                    ResolvedPackageWithVersionReq {
                        version_req: version_req.clone(),
                        package: ResolvedPackage::Missing(name.clone()),
                    },
                );
            }
        }
        Ok(packages)
    }
}

#[instrument]
fn merge_hashmaps<K: Hash + Eq + Clone + fmt::Debug, V: Clone + fmt::Debug>(
    maps: &[&HashMap<K, V>],
) -> HashMap<K, V> {
    let mut result: HashMap<K, V> = HashMap::new();
    for map in maps {
        for (key, value) in map.to_owned() {
            if !result.contains_key(key) {
                result.insert(key.clone(), value.clone());
            }
        }
    }
    result
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
    let base_dir = std::env::current_dir()
        .expect("Failed to get current directory")
        .join(&args.dir);
    let node_modules_path = base_dir.join("node_modules");

    if !node_modules_path.exists() {
        return Err(eyre!(
            "node_modules directory not found in directory: {}",
            base_dir.display()
        ));
    }
    let mut partials = PartialPackage::map_from_node_modules(&node_modules_path)?;

    let root_partial = PartialPackage::from_folder(&base_dir)?;
    let mut workspaces = HashMap::new();
    for glob in &root_partial.workspaces_globs {
        for folder in glob::glob(glob)? {
            let workspace_path = folder?;
            if workspace_path.join("package.json").exists() {
                let partial = PartialPackage::from_folder(&workspace_path)?;
                workspaces.insert(partial.name.clone(), partial);
            }
        }
    }
    partials.extend(workspaces);

    // Create the root package
    let mut resolver = PackageResolver::new(partials);
    let root_package = match resolver.resolve_package(root_partial)? {
        ResolvedPackage::Resolved(package) => package,
        ResolvedPackage::Deduped(_) => {
            return Err(eyre!("Root package is deduped"));
        }
        ResolvedPackage::Missing(_) => {
            return Err(eyre!("Root package is missing"));
        }
    };

    // Display the dependency tree with depth limit
    let config = PrintConfig {
        depth: args.depth as u32,
        ..Default::default()
    };
    ptree::print_tree_with(&root_package, &config).expect("Unable to print dependency tree");

    Ok(())
}
