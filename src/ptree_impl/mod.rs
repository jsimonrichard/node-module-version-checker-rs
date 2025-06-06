mod diff;
mod package;

use std::{borrow::Cow, collections::HashMap, fmt, io};

use colored::*;
use ptree::{Style, TreeItem};

pub(crate) trait ShouldDisplay {
    fn should_display(&self) -> bool {
        true
    }
}

pub(crate) trait Visiting {
    fn visiting(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone)]
pub enum ChildOrDevDependencySeparator<C: TreeItem> {
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

impl<C: TreeItem> TreeItem for ChildOrDevDependencySeparator<C> {
    type Child = C::Child;

    fn write_self<W: io::Write>(&self, f: &mut W, style: &Style) -> io::Result<()> {
        match self {
            Self::Child(child) => child.write_self(f, style),
            Self::DevDependencySeparator => {
                write!(f, "{}", "[DEV DEPENDENCIES]".blue())
            }
        }
    }

    fn children(&self) -> Cow<[C::Child]> {
        if let Self::Child(child) = self {
            child.children()
        } else {
            Cow::Borrowed(&[])
        }
    }
}

impl<C: TreeItem + ShouldDisplay> ShouldDisplay for ChildOrDevDependencySeparator<C> {
    fn should_display(&self) -> bool {
        match self {
            Self::Child(child) => child.should_display(),
            Self::DevDependencySeparator => true,
        }
    }
}

impl<C: TreeItem + Visiting> Visiting for ChildOrDevDependencySeparator<C> {
    fn visiting(&self) -> bool {
        match self {
            Self::Child(child) => child.visiting(),
            Self::DevDependencySeparator => false,
        }
    }
}

pub fn sorted_values<K: Ord + Clone, V: Clone>(deps: &HashMap<K, V>) -> Vec<V> {
    let mut values = deps
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect::<Vec<_>>();
    values.sort_by_cached_key(|(k, _)| k.clone());
    values.into_iter().map(|(_, v)| v).collect()
}
