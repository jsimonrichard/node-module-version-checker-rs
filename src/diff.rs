use std::{
    cell::RefCell,
    collections::HashMap,
    fmt, io,
    rc::{Rc, Weak},
};

use colored::*;
use ptree::PrintConfig;
use semver::Version;

use crate::{
    extended_version_req::ExtendedVersionReq,
    package::{Dependency, Package, PackageEntry, PackageKey},
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
        package: PackageEntry,
        version_req: ExtendedVersionReq,
    },
    Removed {
        package: PackageEntry,
        version_req: ExtendedVersionReq,
    },
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
    Resolved(ChangedPackageKey),
    Missing,
    Truncated,
    MismatchedResolution,
}

enum Side {
    Left,
    Right,
}

impl ChangedPackageEntry {
    fn version_str(&self, side: Side) -> String {
        match self {
            Self::Resolved(package) => package.version_str(side),
            Self::Missing => "[MISSING]".red().to_string(),
            Self::Truncated => "[TRUNCATED]".yellow().to_string(),
            Self::MismatchedResolution => "[MISMATCHED RESOLUTION]".yellow().to_string(),
        }
    }

    fn satisfies(&self, version_req: &ExtendedVersionReq, side: Side) -> Option<bool> {
        match self {
            Self::Resolved(package) => package
                .version(side)
                .as_ref()
                .and_then(|version| version_req.matches(&version)),
            Self::Missing => None,
            Self::Truncated => None,
            Self::MismatchedResolution => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ChangedPackageKey {
    pub left: PackageKey,
    pub right: PackageKey,
}

impl ChangedPackageKey {
    fn version(&self, side: Side) -> &Option<Version> {
        match side {
            Side::Left => &self.left.version,
            Side::Right => &self.right.version,
        }
    }

    fn version_str(&self, side: Side) -> String {
        match self.version(side) {
            Some(version) => version.to_string(),
            None => "{no version}".yellow().to_string(),
        }
    }
}

impl fmt::Display for ChangedPackageKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = if &self.left.name == &self.right.name {
            self.left.name.clone()
        } else {
            format!("({} -> {})", self.left.name, self.right.name)
        };

        let version_str = if &self.left.version == &self.right.version {
            self.version_str(Side::Left)
        } else {
            format!(
                "({} -> {})",
                self.version_str(Side::Left),
                self.version_str(Side::Right)
            )
        };

        write!(f, "{}{}{}", name, "@".bright_black(), version_str)
    }
}

#[derive(Debug, Clone)]
pub struct DiffedPackage {
    pub name: String,
    pub version_left: Option<Version>,
    pub version_right: Option<Version>,
    pub dependencies: HashMap<String, DiffedDependency>,
    pub dev_dependencies: HashMap<String, DiffedDependency>,
    pub(crate) differ: Weak<Differ>,
    pub(crate) visited: RefCell<bool>,
}

impl DiffedPackage {
    fn version(&self, side: Side) -> &Option<Version> {
        match side {
            Side::Left => &self.version_left,
            Side::Right => &self.version_right,
        }
    }

    fn version_str(&self, side: Side) -> String {
        match self.version(side) {
            Some(version) => version.to_string(),
            None => "{no version}".yellow().to_string(),
        }
    }

    fn refresh_visited(&self) {
        if *self.visited.borrow() {
            *self.visited.borrow_mut() = false;
        }
    }

    pub(crate) fn differ(&self) -> Option<Rc<Differ>> {
        self.differ.upgrade()
    }

    pub fn print_tree(&self, config: &PrintConfig) -> io::Result<()> {
        self.differ
            .upgrade()
            .expect("Differ is missing")
            .refresh_visited();
        ptree::print_tree_with(self, config)?;
        Ok(())
    }
}

impl fmt::Display for DiffedPackage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let deduped_str = if *self.visited.borrow() {
            " [DEDUPED]".yellow().to_string()
        } else {
            "".into()
        };

        let version_str = if self.version_left == self.version_right {
            self.version_str(Side::Left)
        } else {
            format!(
                "({} -> {})",
                self.version_str(Side::Left),
                self.version_str(Side::Right)
            )
        };

        write!(
            f,
            "{}{}{}{}",
            self.name,
            "@".bright_black(),
            version_str.blue(),
            deduped_str
        )
    }
}

pub struct Differ {
    diffed_packages: RefCell<HashMap<ChangedPackageKey, Option<Rc<DiffedPackage>>>>,
    left: Rc<Package>,
    right: Rc<Package>,
}

impl Differ {
    pub fn diff(left: Rc<Package>, right: Rc<Package>) -> (Rc<Self>, Option<Rc<DiffedPackage>>) {
        let self_ = Rc::new(Self {
            diffed_packages: RefCell::new(HashMap::new()),
            left,
            right,
        });

        let diffed_package = self_
            .diff_packages(&self_.left, &self_.right)
            .and_then(|weak| weak.upgrade());

        (self_, diffed_package)
    }

    fn diff_packages(
        self: &Rc<Self>,
        left: &Package,
        right: &Package,
    ) -> Option<Weak<DiffedPackage>> {
        let key = ChangedPackageKey {
            left: PackageKey::from(left),
            right: PackageKey::from(right),
        };

        if let Some(diffed_package) = self.diffed_packages.borrow().get(&key) {
            return diffed_package.as_ref().map(|rc| Rc::downgrade(&rc));
        }

        let left = left.clone();
        let right = right.clone();

        let diffed_package = {
            let dependencies = self.diff_dependencies(left.dependencies, right.dependencies);
            let dev_dependencies =
                self.diff_dependencies(left.dev_dependencies, right.dev_dependencies);

            if dependencies.is_empty()
                && dev_dependencies.is_empty()
                && left.version == right.version
                && left.name == right.name
            {
                return None;
            }

            Some(DiffedPackage {
                name: left.name,
                version_left: left.version,
                version_right: right.version,
                dependencies,
                dev_dependencies,
                visited: RefCell::new(false),
                differ: Rc::downgrade(self),
            })
        };

        self.diffed_packages
            .borrow_mut()
            .insert(key.clone(), diffed_package.map(|dp| Rc::new(dp)));

        return self
            .diffed_packages
            .borrow()
            .get(&key)
            .and_then(|dp| dp.as_ref())
            .map(|rc| Rc::downgrade(rc));
    }

    fn diff_dependencies(
        self: &Rc<Self>,
        left: HashMap<String, Dependency>,
        mut right: HashMap<String, Dependency>,
    ) -> HashMap<String, DiffedDependency> {
        let mut dependencies = HashMap::new();

        for (name, left_dep) in left {
            if let Some(right_dep) = right.remove(&name) {
                if let Some(diffed_dependency) = self.diff_dependency(left_dep, right_dep) {
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

    fn diff_dependency(
        self: &Rc<Self>,
        left: Dependency,
        right: Dependency,
    ) -> Option<DiffedDependency> {
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
                package: self.diff_entries(left.package, right.package)?,
            },
        })
    }

    fn diff_entries(
        self: &Rc<Self>,
        left: PackageEntry,
        right: PackageEntry,
    ) -> Option<ChangedPackageEntry> {
        match (left, right) {
            (PackageEntry::Resolved(left), PackageEntry::Resolved(right)) => {
                let left_pkg = self
                    .left
                    .resolver()
                    .expect("Left package is missing")
                    .get_package(&left)?;
                let right_pkg = self
                    .right
                    .resolver()
                    .expect("Right package is missing")
                    .get_package(&right)?;

                // keep the recursion going even though the data structure isn't recursive
                self.diff_packages(&left_pkg, &right_pkg)?;

                Some(ChangedPackageEntry::Resolved(ChangedPackageKey {
                    left: PackageKey::from(&*left_pkg),
                    right: PackageKey::from(&*right_pkg),
                }))
            }
            (PackageEntry::Missing, PackageEntry::Missing) => Some(ChangedPackageEntry::Missing),
            (PackageEntry::Truncated, PackageEntry::Truncated) => {
                Some(ChangedPackageEntry::Truncated)
            }
            _ => Some(ChangedPackageEntry::MismatchedResolution),
        }
    }

    pub(crate) fn get_package(&self, key: &ChangedPackageKey) -> Option<Rc<DiffedPackage>> {
        self.diffed_packages
            .borrow()
            .get(key)
            .and_then(|dp| dp.as_ref())
            .map(|rc| rc.clone())
    }

    fn refresh_visited(&self) {
        for diffed_package in self.diffed_packages.borrow().values().flatten() {
            diffed_package.refresh_visited();
        }
    }
}
