use colored::*;
use ptree::{Style, TreeItem};
use std::{borrow::Cow, fmt, io};

use crate::package::{ResolvedDependency, ResolvedPackage, ResolvedPackageEntry};

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
