use color_eyre::eyre::Result;
use ptree::{Style, TreeItem};
use std::{borrow::Cow, io, rc::Rc};

use crate::package::{Dependency, Package, PackageEntry};

use super::{ChildOrDevDependencySeparator, sorted_values};

#[derive(Debug, Clone)]
pub struct DepWithPackage {
    dependency: Dependency,
    package: Option<Rc<Package>>,
}

impl TreeItem for DepWithPackage {
    type Child = ChildOrDevDependencySeparator<DepWithPackage>;

    fn write_self<W: io::Write>(&self, f: &mut W, style: &Style) -> io::Result<()> {
        if let Some(package) = &self.package {
            write!(f, "{}", style.paint(package))
        } else {
            write!(f, "{}", style.paint(&self.dependency))
        }
    }

    fn children(&self) -> Cow<[Self::Child]> {
        if let Some(package) = &self.package {
            Cow::from(package.children().to_vec())
        } else {
            Cow::Borrowed(&[])
        }
    }
}

impl Package {
    fn populate_children<I: IntoIterator<Item = Dependency>>(
        &self,
        deps: I,
    ) -> Result<Vec<DepWithPackage>> {
        deps.into_iter()
            .map(|d| {
                let package = match &d.package {
                    PackageEntry::Resolved(key) => {
                        self.dep_resolver.upgrade().unwrap().get_package(&key)
                    }
                    _ => None,
                };

                Ok(DepWithPackage {
                    dependency: d,
                    package,
                })
            })
            .collect()
    }
}

impl TreeItem for Package {
    type Child = ChildOrDevDependencySeparator<DepWithPackage>;

    fn write_self<W: io::Write>(&self, f: &mut W, style: &Style) -> io::Result<()> {
        write!(f, "{}", style.paint(self))
    }

    fn children(&self) -> Cow<[Self::Child]> {
        if *self.visited.borrow() {
            return Cow::Borrowed(&[]);
        } else {
            *self.visited.borrow_mut() = true;
        }

        let mut v: Vec<Self::Child> = self
            .populate_children(sorted_values(&self.dependencies))
            .expect("Failed to populate children")
            .into_iter()
            .map(|d| ChildOrDevDependencySeparator::Child(d))
            .collect();

        if !self.dev_dependencies.is_empty() {
            v.push(ChildOrDevDependencySeparator::DevDependencySeparator);
            v.extend(
                self.populate_children(self.dev_dependencies.values().cloned())
                    .expect("Failed to populate children")
                    .into_iter()
                    .map(|d| ChildOrDevDependencySeparator::Child(d)),
            );
        }

        Cow::from(v)
    }
}
