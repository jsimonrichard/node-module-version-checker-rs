use color_eyre::eyre::{Result, eyre};
use colored::*;
use globset::{Glob, GlobSet, GlobSetBuilder};
use semver::Version;
use std::collections::HashMap;
use std::fmt::{self, Display, Formatter};
use std::fs;
use std::hash::Hash;
use std::path::{Path, PathBuf};
use tracing::{instrument, warn};

use crate::extended_version_req::ExtendedVersionReq;

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct PackageKey {
    name: String,
    version: Option<Version>, // Workspace packages may not have a version
}

impl PackageKey {
    fn satisfies(&self, version_req: &ExtendedVersionReq) -> Option<bool> {
        match (version_req, &self.version) {
            (ExtendedVersionReq::SemVer(version_req), Some(version)) => {
                Some(version_req.matches(version))
            }
            _ => None,
        }
    }
}

impl Display for PackageKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if let Some(version) = &self.version {
            write!(f, "{}@{}", self.name, version)
        } else {
            write!(f, "{}", self.name)
        }
    }
}

#[derive(Debug, Clone)]
pub struct PackageJsonData {
    pub name: String,
    pub version: Option<Version>, // allow workspace packages with no versions
    pub install_path: PathBuf,
    pub dependencies: HashMap<String, ExtendedVersionReq>,
    pub dev_dependencies: HashMap<String, ExtendedVersionReq>,
    pub workspaces_globs: Vec<String>,
    workspaces_globset: Option<GlobSet>,
}

impl PackageJsonData {
    #[instrument]
    /// Loads a package from a folder.
    /// Returns None if the folder is not a package.
    /// Returns an Err if the package.json is invalid.
    pub fn from_folder(folder: &Path) -> Result<Option<Self>> {
        let dep_package_json = folder.join("package.json");
        if !dep_package_json.exists() {
            return Ok(None); // Not a package
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
        Ok(Some(Self {
            name,
            version,
            install_path: folder.to_path_buf(),
            dependencies,
            dev_dependencies,
            workspaces_globset: if !workspaces_globs.is_empty() {
                Some(build_globset_from_globs(&workspaces_globs)?)
            } else {
                None
            },
            workspaces_globs,
        }))
    }

    pub fn is_workspace_root(&self) -> bool {
        self.workspaces_globset.is_some()
    }

    pub fn contains_workspace(&self, path: &Path) -> Result<bool> {
        return Ok(self.workspaces_globset.as_ref().unwrap().is_match(path));
    }

    pub fn get_workspaces(&self) -> Result<Vec<PackageJsonData>> {
        let mut workspaces = Vec::new();

        for glob_str in &self.workspaces_globs {
            for folder in glob::glob(glob_str)?.flatten() {
                if let Some(package_data) = PackageJsonData::from_folder(&folder)? {
                    workspaces.push(package_data);
                }
            }
        }

        Ok(workspaces)
    }

    pub fn get_node_modules_path(&self) -> Option<PathBuf> {
        let node_modules_path = self.install_path.join("node_modules");
        if node_modules_path.exists() {
            Some(node_modules_path)
        } else {
            None
        }
    }
}

fn build_globset_from_globs(globs: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for glob in globs {
        builder.add(Glob::new(glob)?);
    }
    return Ok(builder.build()?);
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
pub struct ResolvedDependency {
    pub name: String,
    pub version_req: ExtendedVersionReq,
    pub package: ResolvedPackageEntry,
}

impl ResolvedDependency {
    fn version_mis_match(&self) -> bool {
        !self.package.satisfies(&self.version_req).unwrap_or(true)
    }
}

impl fmt::Display for ResolvedDependency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}{}{} {} {}",
            self.name,
            "@".bright_black(),
            self.version_req.to_string().bright_blue(),
            ":".bright_black(),
            if self.version_mis_match() {
                (self.package.to_string() + " (version not satisfied)")
                    .red()
                    .bold()
            } else {
                self.package.to_string().green()
            }
        )
    }
}

#[derive(Debug, Clone)]
pub enum ResolvedPackageEntry {
    Full(ResolvedPackage),
    Deduped(PackageKey),
    Missing,
}

impl ResolvedPackageEntry {
    fn satisfies(&self, version_req: &ExtendedVersionReq) -> Option<bool> {
        match self {
            Self::Full(package) => package.satisfies(version_req),
            Self::Deduped(key) => key.satisfies(version_req),
            Self::Missing => None,
        }
    }
}

impl fmt::Display for ResolvedPackageEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self {
            Self::Full(package) => {
                if let Some(version) = &package.version {
                    write!(f, "{}", version)
                } else {
                    write!(f, "{}", "{no version}".yellow().italic())
                }
            }
            Self::Deduped(key) => {
                if let Some(version) = &key.version {
                    write!(f, "{} {}", version, "[DEDUPED]".yellow())
                } else {
                    write!(f, "{}", "[DEDUPED]".yellow())
                }
            }
            Self::Missing => {
                write!(f, "{}", "[MISSING]".red())
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedPackage {
    pub name: String,
    pub version: Option<Version>,
    pub install_path: PathBuf,
    pub dependencies: HashMap<String, ResolvedDependency>,
    pub dev_dependencies: HashMap<String, ResolvedDependency>,
}

impl ResolvedPackage {
    fn satisfies(&self, version_req: &ExtendedVersionReq) -> Option<bool> {
        match (version_req, &self.version) {
            (ExtendedVersionReq::SemVer(version_req), Some(version)) => {
                Some(version_req.matches(version))
            }
            _ => None,
        }
    }
}
impl fmt::Display for ResolvedPackage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(version) = &self.version {
            write!(f, "{}@{}", self.name, version)
        } else {
            write!(f, "{}", self.name)
        }
    }
}

#[derive(Debug, Clone)]
pub struct PackageResolver {
    packages_data: HashMap<String, PackageJsonData>,
    resolved_packages: HashMap<PackageKey, ResolvedPackageEntry>,
    visiting: Vec<PackageKey>,
    max_depth: usize,
    current_depth: usize,
}

impl PackageResolver {
    fn new(package_data: HashMap<String, PackageJsonData>, max_depth: usize) -> Self {
        Self {
            packages_data: package_data,
            resolved_packages: HashMap::new(),
            visiting: Vec::new(),
            max_depth,
            current_depth: 0,
        }
    }

    pub(crate) fn from_node_modules(node_modules_path: &Path, max_depth: usize) -> Result<Self> {
        let package_data = package_data_from_node_modules(node_modules_path)?;
        Ok(Self::new(package_data, max_depth))
    }

    pub fn extend(&self, other: &Self) -> Self {
        Self {
            packages_data: merge_hashmaps(&[&other.packages_data, &self.packages_data]),
            resolved_packages: self.resolved_packages.clone(),
            visiting: [self.visiting.clone(), other.visiting.clone()].concat(),
            max_depth: self.max_depth,
            current_depth: self.current_depth,
        }
    }

    #[instrument]
    pub fn resolve_package(
        &mut self,
        package_data: PackageJsonData,
    ) -> Result<ResolvedPackageEntry> {
        let PackageJsonData {
            name,
            version,
            install_path,
            dependencies,
            dev_dependencies,
            workspaces_globs: _,
            workspaces_globset: _,
        } = package_data;

        let key = PackageKey {
            name: name.clone(),
            version: version.clone(),
        };

        if self.visiting.contains(&key) {
            return Ok(ResolvedPackageEntry::Deduped(key));
        }

        if self.current_depth >= self.max_depth {
            return Ok(ResolvedPackageEntry::Full(ResolvedPackage {
                name,
                version,
                install_path: install_path.clone(),
                dependencies: HashMap::new(),
                dev_dependencies: HashMap::new(),
            }));
        }

        if let Some(package) = self.resolved_packages.get(&key) {
            return Ok(package.clone());
        }

        self.visiting.push(key.clone());

        let node_modules_path = install_path.join("node_modules");

        self.current_depth += 1;

        let resolved_dependencies;
        let resolved_dev_dependencies;
        if node_modules_path.exists() {
            let sub_resolver =
                PackageResolver::from_node_modules(&node_modules_path, self.max_depth)?;
            let resolver = &mut self.extend(&sub_resolver);
            resolved_dependencies = resolver.resolve_deps(&dependencies)?;
            resolved_dev_dependencies = resolver.resolve_deps(&dev_dependencies)?;
        } else {
            resolved_dependencies = self.resolve_deps(&dependencies)?;
            resolved_dev_dependencies = self.resolve_deps(&dev_dependencies)?;
        }

        self.current_depth -= 1;

        let package = ResolvedPackageEntry::Full(ResolvedPackage {
            name,
            version,
            install_path: install_path.clone(),
            dependencies: resolved_dependencies,
            dev_dependencies: resolved_dev_dependencies,
        });
        self.resolved_packages.insert(key, package.clone());

        Ok(package)
    }

    #[instrument]
    fn resolve_deps(
        &mut self,
        deps: &HashMap<String, ExtendedVersionReq>,
    ) -> Result<HashMap<String, ResolvedDependency>> {
        let mut packages = HashMap::new();
        for (name, version_req) in deps {
            if let Some(partial) = self.packages_data.get(name).cloned() {
                // A version of that dependency exists
                if let ExtendedVersionReq::Workspace(_) = version_req {
                    // Always dedup workspace dependencies here
                    packages.insert(
                        name.clone(),
                        ResolvedDependency {
                            name: name.clone(),
                            version_req: version_req.clone(),
                            package: ResolvedPackageEntry::Deduped(PackageKey {
                                name: name.clone(),
                                version: None,
                            }),
                        },
                    );
                } else {
                    let package = self.resolve_package(partial)?;

                    packages.insert(
                        name.clone(),
                        ResolvedDependency {
                            name: name.clone(),
                            version_req: version_req.clone(),
                            package,
                        },
                    );
                }
            } else {
                // No version of that dependency exists
                packages.insert(
                    name.clone(),
                    ResolvedDependency {
                        name: name.clone(),
                        version_req: version_req.clone(),
                        package: ResolvedPackageEntry::Missing,
                    },
                );
            }
        }
        Ok(packages)
    }

    pub fn resolve_root_package(
        &mut self,
        root_pkg_data: PackageJsonData,
    ) -> Result<ResolvedPackage> {
        match self.resolve_package(root_pkg_data)? {
            ResolvedPackageEntry::Full(package) => Ok(package),
            ResolvedPackageEntry::Deduped(_) => Err(eyre!("Root package is deduped")),
            ResolvedPackageEntry::Missing => Err(eyre!("Root package is missing")),
        }
    }
}

fn package_data_from_node_modules(
    node_modules_path: &Path,
) -> Result<HashMap<String, PackageJsonData>> {
    let mut pkgs = HashMap::new();
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
                if let Some(package_data) = PackageJsonData::from_folder(&scoped_path)? {
                    pkgs.insert(package_data.name.clone(), package_data);
                }
            }
        } else {
            // Handle regular packages
            if let Some(package_data) = PackageJsonData::from_folder(&path)? {
                pkgs.insert(package_data.name.clone(), package_data);
            }
        }
    }
    Ok(pkgs)
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
