use colored::*;
use ptree::PrintConfig;
use semver::Version;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::{self, Display, Formatter};
use std::hash::Hash;
use std::io;
use std::rc::{Rc, Weak};
use tracing::debug;

use crate::dependency_resolver::DependencyResolver;
use crate::extended_version_req::ExtendedVersionReq;
use crate::package_data::PackageJsonData;

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct PackageKey {
    pub name: String,
    pub version: Option<Version>, // Workspace packages may not have a version
    pub node_modules_id: u32,
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

    fn version_str(&self) -> String {
        self.version
            .as_ref()
            .map(|v| v.to_string())
            .unwrap_or_default()
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

impl From<&PackageJsonData> for PackageKey {
    fn from(package_json_data: &PackageJsonData) -> Self {
        PackageKey {
            name: package_json_data.name.clone(),
            version: package_json_data.version.clone(),
            node_modules_id: package_json_data.parent_id,
        }
    }
}

impl From<&Package> for PackageKey {
    fn from(package: &Package) -> Self {
        Self::from(&package.data)
    }
}

#[derive(Debug, Clone)]
pub struct Dependency {
    pub name: String,
    pub version_req: ExtendedVersionReq,
    pub package: PackageEntry,
}

impl Dependency {
    fn version_mis_match(&self) -> bool {
        !self.package.satisfies(&self.version_req).unwrap_or(true)
    }
}

impl fmt::Display for Dependency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}{}{} {} {}",
            self.name,
            "@".bright_black(),
            self.version_req.to_string().bright_blue(),
            ":".bright_black(),
            if self.version_mis_match() {
                (self.package.version_str() + " (version not satisfied)")
                    .red()
                    .bold()
            } else {
                self.package.version_str().green()
            }
        )
    }
}

#[derive(Debug, Clone)]
pub enum PackageEntry {
    Resolved(PackageKey),
    Missing,
    Truncated,
}

impl PackageEntry {
    pub fn satisfies(&self, version_req: &ExtendedVersionReq) -> Option<bool> {
        match self {
            Self::Resolved(package) => package.satisfies(version_req),
            Self::Missing | Self::Truncated => None,
        }
    }

    pub fn version_str(&self) -> String {
        match self {
            Self::Resolved(package) => package.version_str(),
            Self::Missing => format!("{}", "[MISSING]".red()),
            Self::Truncated => format!("{}", "[TRUNCATED]".yellow()),
        }
    }
}

impl fmt::Display for PackageEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Resolved(package) => package.fmt(f),
            Self::Missing => write!(f, "{}", "[MISSING]".red()),
            Self::Truncated => write!(f, "{}", "[TRUNCATED]".red()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Package {
    pub name: String,
    pub version: Option<Version>,
    pub dependencies: HashMap<String, Dependency>,
    pub dev_dependencies: HashMap<String, Dependency>,
    pub(crate) dep_resolver: Weak<DependencyResolver>,
    pub(crate) visited: RefCell<bool>,
    pub data: PackageJsonData,
}

impl Package {
    pub(crate) fn resolver(&self) -> Option<Rc<DependencyResolver>> {
        self.dep_resolver.upgrade()
    }

    pub fn print_tree(&self, config: &PrintConfig) -> io::Result<()> {
        debug!("Printing tree for {}", self);
        self.resolver()
            .expect("Dependency resolver is missing")
            .refresh_visited();
        ptree::print_tree_with(self, config)?;
        Ok(())
    }
}

impl fmt::Display for Package {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let deduped_text = if *self.visited.borrow() {
            " [DEDUPED]".bright_black()
        } else {
            "".into()
        };
        if let Some(version) = &self.version {
            write!(
                f,
                "{}{}{}{}",
                self.name,
                "@".bright_black(),
                version.to_string().blue(),
                deduped_text
            )
        } else {
            write!(f, "{}{}", self.name, deduped_text)
        }
    }
}
