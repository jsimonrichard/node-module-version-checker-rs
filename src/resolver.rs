use color_eyre::eyre::{Result, eyre};
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use tracing::debug;

use crate::dependency_resolver::DependencyResolver;
use crate::package::Package;
use crate::package_data::{PackageJsonData, get_workspace_globs, read_package_json};
use crate::workspace_data::WorkspaceData;

#[derive(Debug, Clone)]
pub struct WorkspaceRoot {
    pub data: Rc<PackageJsonData>,
    pub dep_resolver: Rc<DependencyResolver>,
}

pub struct Resolver {
    workspace_roots: HashMap<PathBuf, WorkspaceRoot>,
    dependency_resolvers: Vec<Rc<DependencyResolver>>,
    max_depth: usize,
}

/// A factory for creating package data and resolver pairs
impl Resolver {
    pub fn new(max_depth: usize) -> Self {
        Self {
            workspace_roots: HashMap::new(),
            dependency_resolvers: Vec::new(),
            max_depth,
        }
    }

    pub fn get_workspace_root(&self, path: &Path) -> Result<Option<WorkspaceRoot>> {
        let package_path = path.canonicalize()?;
        Ok(self.workspace_roots.get(&package_path).cloned())
    }

    fn search_workspace_root_from(&mut self, path: &Path) -> Result<Option<WorkspaceRoot>> {
        let package_path = path.canonicalize()?;
        let mut current_path = package_path.clone();
        loop {
            if let Some(value) = read_package_json(&current_path.join("package.json"))? {
                debug!("Checking if {} is a workspace root", current_path.display());
                if self.workspace_roots.contains_key(&current_path) {
                    return Ok(Some(self.workspace_roots[&current_path].clone()));
                } else {
                    if let Some(workspaces_globs) = get_workspace_globs(&value, &current_path)? {
                        if package_path != current_path {
                            let globset = build_globset_from_globs(&workspaces_globs)?;
                            if !globset.is_match(&norm_for_glob(&package_path.to_string_lossy())) {
                                continue;
                            }
                        }

                        debug!("Found workspace root: {}", current_path.display());

                        let data = Rc::new(PackageJsonData::new_root(&current_path)?.expect(
                            "failed to create package data even though there's a package.json file",
                        ));
                        let resolver =
                            DependencyResolver::from_folder(current_path.clone(), self.max_depth)?;

                        self.workspace_roots.insert(
                            current_path.clone(),
                            WorkspaceRoot {
                                data,
                                dep_resolver: resolver,
                            },
                        );
                        return Ok(Some(self.workspace_roots[&current_path].clone()));
                    }
                }
            }

            if !current_path.pop() {
                break;
            }
        }
        Ok(None)
    }

    pub fn resolve(&mut self, path: &Path) -> Result<Rc<Package>> {
        let workspace_root = self.search_workspace_root_from(path)?;

        if let Some(workspace_root) = workspace_root {
            let package_data = workspace_root
                .data
                .get_data_from_path(path)
                .expect("package data not found even though we found a workspace root earlier");

            let node_modules_path = package_data.get_node_modules_path().ok_or(eyre!(
                "No node_modules path found in package.json in {}",
                path.display()
            ))?;

            let node_modules = workspace_root
                .dep_resolver
                .root_node_modules
                .get_from_path(&node_modules_path)?;

            let entry = workspace_root
                .dep_resolver
                .resolve_package(&package_data, &node_modules)?;
            let package = workspace_root.dep_resolver.unwrap_entry(entry)?;

            Ok(package)
        } else {
            let package_data = PackageJsonData::new_root(path)?
                .ok_or_else(|| eyre!("No package data found at path {}", path.display()))?;

            let node_modules = package_data
                .get_node_modules()?
                .ok_or_else(|| eyre!("No node_modules found at path {}", path.display()))?;

            let package_resolver = DependencyResolver::new(node_modules, self.max_depth);
            self.dependency_resolvers.push(package_resolver.clone());
            let package = package_resolver.resolve_root_package(&package_data)?;
            Ok(package)
        }
    }

    pub fn resolve_workspace_members(
        &self,
        path: &Path,
        workspace_data: &WorkspaceData,
    ) -> Result<Vec<Rc<Package>>> {
        let workspace_root = self
            .get_workspace_root(path)?
            .ok_or_else(|| eyre!("No workspace root found at {}", path.display()))?;
        let dep_resolver = workspace_root.dep_resolver.clone();

        workspace_data
            .workspace_packages
            .iter()
            .map(|package_data| {
                let node_modules = dep_resolver
                    .root_node_modules
                    .get_from_path(&package_data.install_path)?;

                let entry = dep_resolver.resolve_package(&package_data, &node_modules)?;
                let package = dep_resolver.unwrap_entry(entry)?;
                Ok(package)
            })
            .collect()
    }
}

fn build_globset_from_globs<T: AsRef<str>>(globs: &[T]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for glob in globs {
        builder.add(Glob::new(&norm_for_glob(glob.as_ref()))?);
    }
    return Ok(builder.build()?);
}

fn norm_for_glob(path: &str) -> String {
    path.trim_end_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_norm_for_glob() {
        assert_eq!(norm_for_glob("tests/workspace"), "tests/workspace");
        assert_eq!(norm_for_glob("tests/workspace/"), "tests/workspace");
    }

    #[test]
    fn test_globset() {
        let globs = vec!["/packages/*"];
        let globset = build_globset_from_globs(&globs).unwrap();
        assert!(globset.is_match(&norm_for_glob("/packages/react-vite")));
        assert!(globset.is_match(&norm_for_glob("/packages/react-vite/node_modules")));
    }
}
