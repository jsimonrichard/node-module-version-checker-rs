use std::{borrow::Cow, cell::OnceCell, fmt, io, rc::Rc};

use color_eyre::eyre::Result;
use colored::*;
use ptree::{Style, TreeItem};

use crate::diff::{
    ChangedPackageEntry, DiffedDependency, DiffedPackage, DiffedPackageAndVersionReq,
};

use super::{ChildOrDevDependencySeparator, ShouldDisplay, Visiting, sorted_values};

#[derive(Debug, Clone)]
pub struct DiffedDepWithPackage {
    dependency: DiffedDependency,
    package: Option<Rc<DiffedPackage>>,
    children: OnceCell<Vec<ChildOrDevDependencySeparator<DiffedDepWithPackage>>>,
}

impl DiffedDepWithPackage {
    fn get_children(&self) -> Cow<[ChildOrDevDependencySeparator<DiffedDepWithPackage>]> {
        self.children
            .get_or_init(|| match &self.package {
                Some(package) => package.get_children().to_vec(),
                None => vec![],
            })
            .into()
    }

    fn visited(&self) -> bool {
        match &self.package {
            Some(package) => *package.visited.borrow(),
            None => false,
        }
    }
}

impl fmt::Display for DiffedDepWithPackage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let deduped_str = if self.package.as_ref().map_or(false, |p| *p.visited.borrow()) {
            " [DEDUPED]".bright_black()
        } else {
            "".into()
        };

        write!(f, "{}{}", self.dependency, deduped_str)
    }
}

impl ShouldDisplay for DiffedDepWithPackage {
    fn should_display(&self) -> bool {
        match &self.dependency.package {
            DiffedPackageAndVersionReq::Changed {
                package: ChangedPackageEntry::Resolved(key),
                ..
            } => {
                key.left.name != key.right.name
                    || key.left.version != key.right.version
                    || match &self.package {
                        Some(package) => {
                            if self.visited() {
                                return false;
                            }

                            // Mark the package as visited to avoid infinite recursion
                            *package.visiting.borrow_mut() = true;

                            let res = self
                                .get_children()
                                .iter()
                                .filter(|c| !c.visiting())
                                .any(|c| c.should_display());

                            // Reset the visited flag
                            *package.visiting.borrow_mut() = false;

                            res
                        }
                        None => false,
                    }
            }
            _ => true,
        }
    }
}

impl Visiting for DiffedDepWithPackage {
    fn visiting(&self) -> bool {
        match &self.package {
            Some(package) => *package.visiting.borrow(),
            None => false,
        }
    }
}

impl TreeItem for DiffedDepWithPackage {
    type Child = ChildOrDevDependencySeparator<DiffedDepWithPackage>;

    fn write_self<W: io::Write>(&self, f: &mut W, style: &Style) -> io::Result<()> {
        write!(f, "{}", style.paint(self))
    }

    fn children(&self) -> Cow<[Self::Child]> {
        if let Some(package) = &self.package {
            if *package.visited.borrow() {
                return Cow::Borrowed(&[]);
            } else {
                *package.visited.borrow_mut() = true;
            }
        }

        self.get_children()
            .into_iter()
            .cloned()
            .filter(|c| c.should_display())
            .collect()
    }
}

impl DiffedPackage {
    fn populate_children<I: IntoIterator<Item = DiffedDependency>>(
        &self,
        deps: I,
    ) -> Result<Vec<DiffedDepWithPackage>> {
        deps.into_iter()
            .map(|d| {
                let package = match &d.package {
                    DiffedPackageAndVersionReq::Changed {
                        package: ChangedPackageEntry::Resolved(key),
                        ..
                    } => self
                        .differ()
                        .expect("Failed to get differ")
                        .get_package(&key),
                    _ => None,
                };

                Ok(DiffedDepWithPackage {
                    dependency: d.clone(),
                    package,
                    children: OnceCell::new(),
                })
            })
            .collect()
    }

    fn get_children(&self) -> Cow<[ChildOrDevDependencySeparator<DiffedDepWithPackage>]> {
        let mut v: Vec<ChildOrDevDependencySeparator<DiffedDepWithPackage>> = self
            .populate_children(sorted_values(&self.dependencies))
            .expect("Failed to populate children")
            .into_iter()
            .map(|r| ChildOrDevDependencySeparator::Child(r))
            .collect();

        if !self.dev_dependencies.is_empty() {
            v.push(ChildOrDevDependencySeparator::DevDependencySeparator);
            v.extend(
                self.populate_children(sorted_values(&self.dev_dependencies))
                    .expect("Failed to populate children")
                    .into_iter()
                    .map(|r| ChildOrDevDependencySeparator::Child(r)),
            );
        }

        v.into()
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

        self.get_children()
            .into_iter()
            .cloned()
            .filter(|c| c.should_display())
            .collect()
    }
}
