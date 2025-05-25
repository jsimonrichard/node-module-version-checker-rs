use color_eyre::eyre::{Result, eyre};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::package::{PackageJsonData, PackageResolver};

pub struct WorkspaceResolver {
    workspace_roots: HashMap<PathBuf, (PackageJsonData, PackageResolver)>,
    max_depth: usize,
}

impl WorkspaceResolver {
    pub fn new(max_depth: usize) -> Self {
        Self {
            workspace_roots: HashMap::new(),
            max_depth,
        }
    }

    /// Get the workspace root's associated data
    fn get_workspace_root_data(
        &mut self,
        root_path: PathBuf,
    ) -> Result<&(PackageJsonData, PackageResolver)> {
        if self.workspace_roots.contains_key(&root_path) {
            return Ok(&self.workspace_roots[&root_path]);
        }

        let package_data = PackageJsonData::from_folder(&root_path)?
            .ok_or(eyre!("No package.json found for {}", root_path.display()))?;
        if package_data.workspaces_globs.is_empty() {
            return Err(eyre!(
                "No workspaces found in package.json for {}",
                root_path.display()
            ));
        }

        let package_resolver = self
            .create_immediate_package_resolver(&package_data)?
            .ok_or(eyre!("No node_modules found for {}", root_path.display()))?;
        self.workspace_roots
            .insert(root_path.clone(), (package_data, package_resolver));

        Ok(&self.workspace_roots[&root_path])
    }

    /// Create a package resolver for the immediate node_modules folder
    fn create_immediate_package_resolver(
        &self,
        package_data: &PackageJsonData,
    ) -> Result<Option<PackageResolver>> {
        if let Some(node_modules_path) = package_data.get_node_modules_path() {
            Some(PackageResolver::from_node_modules(
                &node_modules_path,
                self.max_depth,
            ))
            .transpose()
        } else {
            Ok(None)
        }
    }

    /// Get a package resolver (including packages from the workspace, if applicable)
    /// for a package
    pub fn get_package_resolver(
        &mut self,
        package_data: &PackageJsonData,
    ) -> Result<PackageResolver> {
        if package_data.is_workspace_root() {
            return Ok(self
                .get_workspace_root_data(package_data.install_path.clone())?
                .1
                .clone());
        }

        let immediate_resolver = self.create_immediate_package_resolver(package_data)?;
        let root_resolver = resolve_workspace_root(&package_data.install_path)?
            .map(|p| self.get_workspace_root_data(p.install_path))
            .transpose()?
            .map(|(_, resolver)| resolver);

        match (immediate_resolver, root_resolver) {
            (Some(immediate_resolver), Some(root_resolver)) => {
                Ok(immediate_resolver.extend(&root_resolver))
            }
            (Some(immediate_resolver), None) => Ok(immediate_resolver),
            (None, Some(root_resolver)) => Ok(root_resolver.clone()),
            (None, None) => Err(eyre!(
                "No workspace root found for {}",
                package_data.install_path.display()
            )),
        }
    }
}

fn resolve_workspace_root(path: &Path) -> Result<Option<PackageJsonData>> {
    let mut path_buf = path.to_path_buf();
    loop {
        let package_data_opt = PackageJsonData::from_folder(&path_buf)?;
        if let Some(package_data) = package_data_opt {
            if !package_data.workspaces_globs.is_empty() {
                if package_data.contains_workspace(&path_buf)? {
                    return Ok(Some(package_data));
                }
            }
        }

        if !path_buf.pop() {
            break;
        }
    }
    Ok(None)
}
