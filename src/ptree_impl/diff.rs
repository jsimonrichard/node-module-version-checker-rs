use std::{borrow::Cow, fmt, io, rc::Rc};

use color_eyre::eyre::Result;
use ptree::{Style, TreeItem};

use crate::diff::{
    ChangedPackageEntry, DiffedDependency, DiffedPackage, DiffedPackageAndVersionReq,
};

use super::ChildOrDevDependencySeparator;

#[derive(Debug, Clone)]
pub struct DiffedDepWithPackage {
    dependency: DiffedDependency,
    package: Option<Rc<DiffedPackage>>,
}

impl fmt::Display for DiffedDepWithPackage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.dependency)
    }
}

impl TreeItem for DiffedDepWithPackage {
    type Child = ChildOrDevDependencySeparator<DiffedDepWithPackage>;

    fn write_self<W: io::Write>(&self, f: &mut W, style: &Style) -> io::Result<()> {
        write!(f, "{}", style.paint(self))
    }

    fn children(&self) -> Cow<[Self::Child]> {
        match &self.package {
            Some(package) => Cow::from(package.children().to_vec()),
            None => Cow::Borrowed(&[]),
        }
    }
}

impl DiffedPackage {
    fn populate_children<I: IntoIterator<Item = DiffedDependency>>(
        &self,
        deps: I,
    ) -> Result<Vec<DiffedDepWithPackage>> {
        deps.into_iter()
            .map(|d| match &d.package {
                DiffedPackageAndVersionReq::Changed {
                    package: ChangedPackageEntry::Resolved(key),
                    ..
                } => {
                    let package = self.differ().unwrap().get_package(&key).unwrap();
                    Ok(DiffedDepWithPackage {
                        dependency: d.clone(),
                        package: Some(package),
                    })
                }
                _ => Ok(DiffedDepWithPackage {
                    dependency: d.clone(),
                    package: None,
                }),
            })
            .collect()
    }
}

impl TreeItem for DiffedPackage {
    type Child = ChildOrDevDependencySeparator<DiffedDepWithPackage>;

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
            .populate_children(self.dependencies.values().cloned())
            .expect("Failed to populate children")
            .into_iter()
            .map(|r| ChildOrDevDependencySeparator::Child(r))
            .collect();

        if !self.dev_dependencies.is_empty() {
            v.push(ChildOrDevDependencySeparator::DevDependencySeparator);
            v.extend(
                self.populate_children(self.dev_dependencies.values().cloned())
                    .expect("Failed to populate children")
                    .into_iter()
                    .map(|r| ChildOrDevDependencySeparator::Child(r)),
            );
        }
        Cow::from(v)
    }
}
