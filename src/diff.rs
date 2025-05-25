use std::{collections::HashMap, fmt};

use colored::*;
use semver::Version;

use crate::{
    extended_version_req::ExtendedVersionReq,
    package::{PackageKey, ResolvedDependency, ResolvedPackage, ResolvedPackageEntry},
};

#[derive(Debug, Clone)]
pub struct DiffedDependency {
    pub name: String,
    pub package: DiffedPackageAndVersionReq,
}

#[derive(Debug, Clone)]
pub enum DiffedPackageAndVersionReq {
    Changed {
        package: ChangedPackageEntry,
        version_req_left: ExtendedVersionReq,
        version_req_right: ExtendedVersionReq,
    },
    Added {
        package: ResolvedPackageEntry,
        version_req: ExtendedVersionReq,
    },
    Removed {
        package: ResolvedPackageEntry,
        version_req: ExtendedVersionReq,
    },
}

impl DiffedDependency {
    pub fn from(left: ResolvedDependency, right: ResolvedDependency) -> Option<Self> {
        let name = if left.name != right.name {
            format!("({} -> {})", left.name, right.name)
        } else {
            left.name
        };

        Some(DiffedDependency {
            name,
            package: DiffedPackageAndVersionReq::Changed {
                version_req_left: left.version_req,
                version_req_right: right.version_req,
                package: ChangedPackageEntry::from(left.package, right.package)?,
            },
        })
    }
}

impl fmt::Display for DiffedDependency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let prefix = match &self.package {
            DiffedPackageAndVersionReq::Changed { .. } => "".to_string(),
            DiffedPackageAndVersionReq::Added { .. } => "[ADDED] ".green().to_string(),
            DiffedPackageAndVersionReq::Removed { .. } => "[REMOVED] ".red().to_string(),
        };

        let version_req_str = match &self.package {
            DiffedPackageAndVersionReq::Changed {
                version_req_left,
                version_req_right,
                ..
            } => {
                if version_req_left != version_req_right {
                    format!("({} -> {})", version_req_left, version_req_right)
                } else {
                    version_req_left.to_string()
                }
            }
            DiffedPackageAndVersionReq::Added { version_req, .. }
            | DiffedPackageAndVersionReq::Removed { version_req, .. } => version_req.to_string(),
        };

        let version_str = match &self.package {
            DiffedPackageAndVersionReq::Changed {
                package,
                version_req_left,
                version_req_right,
            } => {
                let left = match package.satisfies(&version_req_left, Side::Left) {
                    Some(true) => package.version_str(Side::Left).green(),
                    Some(false) => package.version_str(Side::Left).red(),
                    None => package.version_str(Side::Left).into(),
                };
                let right = match package.satisfies(&version_req_right, Side::Right) {
                    Some(true) => package.version_str(Side::Right).green(),
                    Some(false) => package.version_str(Side::Right).red(),
                    None => package.version_str(Side::Right).into(),
                };
                if left != right {
                    format!("{} -> {}", left, right)
                } else {
                    left.to_string()
                }
            }
            DiffedPackageAndVersionReq::Added {
                version_req,
                package,
            }
            | DiffedPackageAndVersionReq::Removed {
                version_req,
                package,
            } => match package.satisfies(&version_req) {
                Some(true) => package.to_string().green().to_string(),
                Some(false) => package.to_string().red().to_string(),
                None => package.to_string(),
            },
        };

        write!(
            f,
            "{}{}{}{} {} {}",
            prefix,
            self.name,
            "@".bright_black(),
            version_req_str.bright_blue(),
            ":".bright_black(),
            version_str
        )
    }
}

#[derive(Debug, Clone)]
pub enum ChangedPackageEntry {
    Full(DiffedPackage),
    Deduped(ChangedPackageKey),
    Missing,
    MismatchedResolution,
}

enum Side {
    Left,
    Right,
}

impl ChangedPackageEntry {
    pub fn from(left: ResolvedPackageEntry, right: ResolvedPackageEntry) -> Option<Self> {
        match (left, right) {
            (ResolvedPackageEntry::Full(left), ResolvedPackageEntry::Full(right)) => {
                Some(Self::Full(DiffedPackage::from(left, right)?))
            }
            (ResolvedPackageEntry::Deduped(left), ResolvedPackageEntry::Deduped(right)) => {
                Some(Self::Deduped(ChangedPackageKey::from(left, right)?))
            }
            (ResolvedPackageEntry::Missing, ResolvedPackageEntry::Missing) => Some(Self::Missing),
            _ => Some(Self::MismatchedResolution),
        }
    }

    fn version_str(&self, side: Side) -> String {
        match self {
            Self::Full(package) => {
                if let Some(version) = package.version(side) {
                    format!("{}", version)
                } else {
                    format!("{}", "{no version}".yellow().italic())
                }
            }
            Self::Deduped(key) => {
                if let Some(version) = key.version(side) {
                    format!("{} {}", version, "[DEDUPED]".yellow())
                } else {
                    format!(
                        "{} {}",
                        "{no version}".yellow().italic(),
                        "[DEDUPED]".yellow()
                    )
                }
            }
            Self::Missing => "[MISSING]".red().to_string(),
            Self::MismatchedResolution => "[MISMATCHED RESOLUTION]".yellow().to_string(),
        }
    }

    fn satisfies(&self, version_req: &ExtendedVersionReq, side: Side) -> Option<bool> {
        match self {
            Self::Full(package) => package.satisfies(version_req, side),
            Self::Deduped(key) => key.satisfies(version_req, side),
            Self::Missing => None,
            Self::MismatchedResolution => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChangedPackageKey {
    pub name: String,
    pub version_left: Option<Version>,
    pub version_right: Option<Version>,
}

impl ChangedPackageKey {
    pub fn from(left: PackageKey, right: PackageKey) -> Option<Self> {
        if left == right {
            return None;
        }

        Some(Self {
            name: if left.name != right.name {
                format!("{} -> {}", left.name, right.name)
            } else {
                left.name
            },
            version_left: left.version,
            version_right: right.version,
        })
    }

    fn version(&self, side: Side) -> &Option<Version> {
        match side {
            Side::Left => &self.version_left,
            Side::Right => &self.version_right,
        }
    }

    fn satisfies(&self, version_req: &ExtendedVersionReq, side: Side) -> Option<bool> {
        match version_req {
            ExtendedVersionReq::SemVer(version_req) => self
                .version(side)
                .as_ref()
                .map(|version| version_req.matches(&version)),
            _ => None,
        }
    }
}

impl fmt::Display for ChangedPackageKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.version_left, &self.version_right) {
            (Some(left), Some(right)) => {
                if left != right {
                    write!(f, "{}@({} -> {})", self.name, left, right)
                } else {
                    write!(f, "{}@{}", self.name, left)
                }
            }
            (Some(left), None) => write!(f, "{}@({} -> None)", self.name, left),
            (None, Some(right)) => write!(f, "{}@(None -> {})", self.name, right),
            (None, None) => write!(f, "{}", self.name),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DiffedPackage {
    pub name: String,
    pub version_left: Option<Version>,
    pub version_right: Option<Version>,
    pub dependencies: HashMap<String, DiffedDependency>,
    pub dev_dependencies: HashMap<String, DiffedDependency>,
}

impl DiffedPackage {
    pub fn from(left: ResolvedPackage, right: ResolvedPackage) -> Option<Self> {
        let dependencies = diff_dependencies(left.dependencies, right.dependencies);
        let dev_dependencies = diff_dependencies(left.dev_dependencies, right.dev_dependencies);

        if dependencies.is_empty()
            && dev_dependencies.is_empty()
            && left.version == right.version
            && left.name == right.name
        {
            return None;
        }

        Some(Self {
            name: left.name,
            version_left: left.version,
            version_right: right.version,
            dependencies,
            dev_dependencies,
        })
    }

    fn version(&self, side: Side) -> &Option<Version> {
        match side {
            Side::Left => &self.version_left,
            Side::Right => &self.version_right,
        }
    }

    fn satisfies(&self, version_req: &ExtendedVersionReq, side: Side) -> Option<bool> {
        self.version(side)
            .as_ref()
            .and_then(|version| version_req.matches(&version))
    }
}

impl fmt::Display for DiffedPackage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.version_left, &self.version_right) {
            (Some(left), Some(right)) => {
                if left != right {
                    write!(f, "{}@({} -> {})", self.name, left, right)
                } else {
                    write!(f, "{}@{}", self.name, left)
                }
            }
            (Some(left), None) => write!(f, "{}@({} -> None)", self.name, left),
            (None, Some(right)) => write!(f, "{}@(None -> {})", self.name, right),
            (None, None) => write!(f, "{}", self.name),
        }
    }
}

fn diff_dependencies(
    left: HashMap<String, ResolvedDependency>,
    mut right: HashMap<String, ResolvedDependency>,
) -> HashMap<String, DiffedDependency> {
    let mut dependencies = HashMap::new();

    for (name, left_dep) in left {
        if let Some(right_dep) = right.remove(&name) {
            if let Some(diffed_dependency) = DiffedDependency::from(left_dep, right_dep) {
                dependencies.insert(name, diffed_dependency);
            }
        } else {
            dependencies.insert(
                name.clone(),
                DiffedDependency {
                    name,
                    package: DiffedPackageAndVersionReq::Removed {
                        package: left_dep.package,
                        version_req: left_dep.version_req,
                    },
                },
            );
        }
    }

    for (name, right_dep) in right {
        dependencies.insert(
            name.clone(),
            DiffedDependency {
                name,
                package: DiffedPackageAndVersionReq::Added {
                    package: right_dep.package,
                    version_req: right_dep.version_req,
                },
            },
        );
    }

    return dependencies;
}
