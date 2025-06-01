use std::{borrow::Cow, cell::OnceCell, fmt, io, rc::Rc};

use color_eyre::eyre::Result;
use ptree::{Style, TreeItem};

use crate::diff::{
    ChangedPackageEntry, DiffedDependency, DiffedPackage, DiffedPackageAndVersionReq,
};

use super::{ChildOrDevDependencySeparator, ShouldDisplay};

#[derive(Debug, Clone)]
pub struct DiffedDepWithPackage {
    dependency: DiffedDependency,
    package: Option<Rc<DiffedPackage>>,
    children: OnceCell<Vec<ChildOrDevDependencySeparator<DiffedDepWithPackage>>>,
    _should_display: OnceCell<bool>,
}

impl DiffedDepWithPackage {
    fn get_children(&self) -> Cow<[ChildOrDevDependencySeparator<DiffedDepWithPackage>]> {
        Cow::from(self.children.get_or_init(|| match &self.package {
            Some(package) => package.get_children().to_vec(),
            None => vec![],
        }))
    }
}

impl fmt::Display for DiffedDepWithPackage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(package) = &self.package {
            write!(f, "{}", package)
        } else {
            write!(f, "{}", self.dependency)
        }
    }
}

impl ShouldDisplay for DiffedDepWithPackage {
    fn should_display(&self) -> bool {
        *self
            ._should_display
            .get_or_init(|| match &self.dependency.package {
                DiffedPackageAndVersionReq::Changed {
                    package: ChangedPackageEntry::Resolved(key),
                    ..
                } => {
                    key.left.name != key.right.name
                        || key.left.version != key.right.version
                        || match &self.package {
                            Some(_) => self.get_children().iter().any(|c| c.should_display()),
                            None => false,
                        }
                }
                _ => true,
            })
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

        let children = self.get_children();
        children
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
                    _should_display: OnceCell::new(),
                })
            })
            .collect()
    }

    fn get_children(&self) -> Cow<[ChildOrDevDependencySeparator<DiffedDepWithPackage>]> {
        let mut v: Vec<ChildOrDevDependencySeparator<DiffedDepWithPackage>> = self
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

        v.into_iter().filter(|c| c.should_display()).collect()
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
    }
}
