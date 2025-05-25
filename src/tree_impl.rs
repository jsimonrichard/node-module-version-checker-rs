use colored::*;
use ptree::{Style, TreeItem};
use std::{borrow::Cow, fmt, io};

use crate::{
    diff::{ChangedPackageEntry, DiffedDependency, DiffedPackage, DiffedPackageAndVersionReq},
    package::{ResolvedDependency, ResolvedPackage, ResolvedPackageEntry},
};

impl TreeItem for ResolvedPackage {
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
pub enum PackageTreeChild {
    Package(ResolvedDependency),
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
        if let PackageTreeChild::Package(ResolvedDependency {
            package: ResolvedPackageEntry::Full(package),
            ..
        }) = self
        {
            Cow::from(package.children().to_vec())
        } else {
            Cow::Borrowed(&[])
        }
    }
}

impl TreeItem for DiffedPackage {
    type Child = DiffedDependencyChild;

    fn write_self<W: io::Write>(&self, f: &mut W, style: &Style) -> io::Result<()> {
        write!(f, "{}", style.paint(self))
    }

    fn children(&self) -> Cow<[Self::Child]> {
        let mut v: Vec<DiffedDependencyChild> = self
            .dependencies
            .values()
            .map(|r| DiffedDependencyChild::Package(r.clone()))
            .collect();

        if !self.dev_dependencies.is_empty() {
            v.push(DiffedDependencyChild::DevDependencySeparator);
            v.extend(
                self.dev_dependencies
                    .values()
                    .map(|r| DiffedDependencyChild::Package(r.clone())),
            );
        }
        Cow::from(v)
    }
}

#[derive(Debug, Clone)]
pub enum DiffedDependencyChild {
    Package(DiffedDependency),
    DevDependencySeparator,
}

impl fmt::Display for DiffedDependencyChild {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiffedDependencyChild::Package(package) => write!(f, "{}", package),
            DiffedDependencyChild::DevDependencySeparator => {
                write!(f, "{}", "[DEV DEPENDENCIES]".blue())
            }
        }
    }
}

impl TreeItem for DiffedDependencyChild {
    type Child = DiffedDependencyChild;

    fn write_self<W: io::Write>(&self, f: &mut W, style: &Style) -> io::Result<()> {
        write!(f, "{}", style.paint(self))
    }

    fn children(&self) -> Cow<[Self::Child]> {
        match self {
            DiffedDependencyChild::Package(package) => Cow::from(package.children().to_vec()),
            DiffedDependencyChild::DevDependencySeparator => Cow::Borrowed(&[]),
        }
    }
}

impl TreeItem for DiffedDependency {
    type Child = DiffedDependencyChild;

    fn write_self<W: io::Write>(&self, f: &mut W, style: &Style) -> io::Result<()> {
        write!(f, "{}", style.paint(self))
    }

    fn children(&self) -> Cow<[Self::Child]> {
        match &self.package {
            DiffedPackageAndVersionReq::Changed {
                package: ChangedPackageEntry::Full(package),
                ..
            } => Cow::from(package.children().to_vec()),
            _ => Cow::Borrowed(&[]),
        }
    }
}
