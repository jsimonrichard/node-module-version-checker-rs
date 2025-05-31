use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
    sync::atomic::Ordering,
};

use color_eyre::eyre::{Report, Result, eyre};
use colored::*;
use semver::Version;
use serde_json::Value;

use crate::{
    extended_version_req::ExtendedVersionReq,
    node_modules::{NEXT_PARENT_ID, NodeModules},
    workspace_data::WorkspaceData,
};

#[derive(Debug, Clone)]
pub struct PackageJsonData {
    pub name: String,
    pub version: Option<Version>, // allow workspace packages with no versions
    pub install_path: PathBuf,
    pub parent_id: u32,
    pub dependencies: HashMap<String, ExtendedVersionReq>,
    pub dev_dependencies: HashMap<String, ExtendedVersionReq>,
    pub workspace_data: Option<WorkspaceData>,
}

impl PackageJsonData {
    /// Loads a package from a folder .
    /// Returns None if the folder is not a package.
    /// Returns an Err if the package.json is invalid.
    pub fn new_root(folder: &Path) -> Result<Option<Self>> {
        let id = NEXT_PARENT_ID.fetch_add(1, Ordering::SeqCst);
        return Self::from_folder_with_id(folder, id);
    }

    pub(crate) fn from_folder_with_id(folder: &Path, node_modules_id: u32) -> Result<Option<Self>> {
        if let Some(value) = read_package_json(&folder.join("package.json"))? {
            return Self::from_value(value, node_modules_id, folder);
        } else {
            return Ok(None);
        }
    }

    pub(crate) fn from_value(
        dep_json: serde_json::Value,
        node_modules_id: u32,
        install_path: &Path,
    ) -> Result<Option<Self>> {
        let name = dep_json
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or(&"{no name}".yellow())
            .to_string();
        let version = dep_json
            .get("version")
            .and_then(|v| v.as_str())
            .map(|v| Version::parse(v))
            .transpose()?;
        let dependencies = dep_json
            .get("dependencies")
            .map(deps_from_value)
            .transpose()?
            .unwrap_or_default();

        // Only load devDependencies if the package is not in node_modules
        let dev_dependencies = if install_path.to_string_lossy().contains("node_modules") {
            HashMap::new()
        } else {
            dep_json
                .get("devDependencies")
                .map(deps_from_value)
                .transpose()?
                .unwrap_or_default()
        };

        let install_path = install_path.canonicalize()?;

        let globs = get_workspace_globs(&dep_json, &install_path)?;
        let workspace_data = globs
            .map(|globs| WorkspaceData::from_globs(&globs))
            .transpose()?;

        Ok(Some(Self {
            name,
            version,
            install_path,
            parent_id: node_modules_id,
            dependencies,
            dev_dependencies,
            workspace_data,
        }))
    }

    pub fn get_node_modules_path(&self) -> Option<PathBuf> {
        let node_modules_path = self.install_path.join("node_modules");
        if node_modules_path.exists() {
            Some(node_modules_path)
        } else {
            None
        }
    }

    pub fn get_node_modules(&self) -> Result<Option<Rc<NodeModules>>> {
        self.get_node_modules_path()
            .map(|path| NodeModules::from_folder(path))
            .transpose()
    }

    pub fn get_data_from_path(self: Rc<Self>, path: &Path) -> Option<Rc<PackageJsonData>> {
        let path = path.canonicalize().ok()?;
        if path == self.install_path {
            Some(self.clone())
        } else {
            self.workspace_data
                .as_ref()
                .and_then(|workspace_data| workspace_data.get_member_data_from_path(&path))
        }
    }

    pub fn is_workspace_root(&self) -> bool {
        self.workspace_data.is_some()
    }
}

pub(crate) fn read_package_json(path: &Path) -> Result<Option<Value>> {
    if !path.exists() {
        return Ok(None); // Not a package
    }
    let content = fs::read_to_string(path)?;
    let dep_json: serde_json::Value = serde_json::from_str(&content)?;
    Ok(Some(dep_json))
}

fn deps_from_value(deps: &serde_json::Value) -> Result<HashMap<String, ExtendedVersionReq>> {
    let mut result = HashMap::new();
    let deps_object = deps
        .as_object()
        .ok_or(eyre!("dependencies is not an object"))?;
    for (name, version) in deps_object {
        let version_str = version.as_str().ok_or(eyre!("version is not a string"))?;
        let version_req = ExtendedVersionReq::parse(version_str);
        result.insert(name.clone(), version_req);
    }
    Ok(result)
}

pub fn get_workspace_globs(value: &Value, install_path: &Path) -> Result<Option<Vec<String>>> {
    Ok(value
        .get("workspaces")
        .map(|w| {
            Ok::<_, Report>(
                w.as_array()
                    .ok_or_else(|| {
                        eyre!(
                            "workspaces in package.json in {} is not an array",
                            install_path.display()
                        )
                    })?
                    .iter()
                    .map(|w| {
                        Ok(w.as_str().ok_or_else(|| {
                            eyre!(
                                "workspace entry in package.json in {} is not a string",
                                install_path.display()
                            )
                        })?)
                    })
                    .collect::<Result<Vec<&str>>>()?
                    .into_iter()
                    .filter(|w| *w != ".")
                    .map(|w| install_path.to_string_lossy().to_string() + "/" + w)
                    .collect::<Vec<String>>(),
            )
        })
        .transpose()?
        .and_then(|v| if v.is_empty() { None } else { Some(v) }))
}
