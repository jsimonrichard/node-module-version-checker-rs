use std::{path::Path, rc::Rc, sync::atomic::Ordering};

use color_eyre::eyre::Result;
use tracing::debug;

use crate::{node_modules::NEXT_PARENT_ID, package_data::PackageJsonData};

#[derive(Debug, Clone)]
pub struct WorkspaceData {
    // workspace roots kind of work like a node_modules folder (it's a parent),
    // so we'll make an id for them
    // id: u32,
    pub workspace_packages: Vec<Rc<PackageJsonData>>,
}

impl WorkspaceData {
    pub fn from_globs(globs: &[String]) -> Result<Self> {
        let id = NEXT_PARENT_ID.fetch_add(1, Ordering::SeqCst);
        let workspace_packages = get_workspace_packages(globs, id)?;

        Ok(Self {
            workspace_packages: workspace_packages
                .iter()
                .map(|p| Rc::new(p.clone()))
                .collect(),
        })
    }

    pub fn get_member_data_from_path(&self, path: &Path) -> Option<Rc<PackageJsonData>> {
        let path = path.canonicalize().ok()?;
        self.workspace_packages
            .iter()
            .find(|p| {
                p.install_path
                    .canonicalize()
                    .ok()
                    .map(|p| p == path)
                    .unwrap_or(false)
            })
            .map(|p| p.clone())
    }
}

fn get_workspace_packages(globs: &[String], parent_id: u32) -> Result<Vec<PackageJsonData>> {
    let mut workspace_packages = Vec::new();

    for glob in globs {
        for folder in glob::glob(glob)?.flatten() {
            debug!("Found potential workspace package: {}", folder.display());
            if let Some(package_data) = PackageJsonData::from_folder_with_id(&folder, parent_id)? {
                workspace_packages.push(package_data);
            }
        }
    }

    Ok(workspace_packages)
}
