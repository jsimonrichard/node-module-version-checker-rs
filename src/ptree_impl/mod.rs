mod diff;
mod package;

use std::{borrow::Cow, fmt, io};

use colored::*;
use ptree::{Style, TreeItem};

#[derive(Debug, Clone)]
pub enum ChildOrDevDependencySeparator<C: TreeItem + fmt::Display> {
    Child(C),
    DevDependencySeparator,
}

impl<C: fmt::Display + TreeItem> fmt::Display for ChildOrDevDependencySeparator<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Child(child) => write!(f, "{}", child),
            Self::DevDependencySeparator => {
                write!(f, "{}", "[DEV DEPENDENCIES]".blue())
            }
        }
    }
}

impl<C: TreeItem + fmt::Display> TreeItem for ChildOrDevDependencySeparator<C> {
    type Child = C::Child;

    fn write_self<W: io::Write>(&self, f: &mut W, style: &Style) -> io::Result<()> {
        write!(f, "{}", style.paint(self))
    }

    fn children(&self) -> Cow<[C::Child]> {
        if let Self::Child(child) = self {
            child.children()
        } else {
            Cow::Borrowed(&[])
        }
    }
}
