use std::{cell::RefCell, collections::HashMap, path::PathBuf, rc::Rc};

use color_eyre::eyre::{Result, eyre};

use crate::{
    extended_version_req::ExtendedVersionReq,
    node_modules::NodeModules,
    package::{Dependency, Package, PackageEntry, PackageKey},
    package_data::PackageJsonData,
};

#[derive(Debug, Clone)]
pub struct DependencyResolver {
    pub(crate) root_node_modules: Rc<NodeModules>,
    max_depth: usize,
    packages: RefCell<HashMap<PackageKey, Rc<Package>>>,
    visiting: RefCell<Vec<PackageKey>>,
    current_depth: RefCell<usize>,
}

impl DependencyResolver {
    pub fn new(root_node_modules: Rc<NodeModules>, max_depth: usize) -> Rc<Self> {
        Rc::new(Self {
            root_node_modules,
            max_depth,
            packages: RefCell::new(HashMap::new()),
            visiting: RefCell::new(Vec::new()),
            current_depth: RefCell::new(0),
        })
    }

    pub(crate) fn from_folder(folder: PathBuf, max_depth: usize) -> Result<Rc<Self>> {
        let node_modules_path = folder.join("node_modules");
        let node_modules = NodeModules::from_folder(node_modules_path)?;
        Ok(Self::new(node_modules, max_depth))
    }

    pub fn resolve_package(
        self: &Rc<Self>,
        package_data: &PackageJsonData,
        node_modules: &Rc<NodeModules>,
    ) -> Result<PackageEntry> {
        let PackageJsonData {
            name,
            version,
            install_path,
            dependencies,
            dev_dependencies,
            ..
        } = package_data;

        let key = package_data.into();

        if self.packages.borrow().contains_key(&key) || self.visiting.borrow().contains(&key) {
            return Ok(PackageEntry::Resolved(key));
        }

        if *self.current_depth.borrow() > self.max_depth {
            return Ok(PackageEntry::Truncated);
        }

        let resolved_dependencies;
        let resolved_dev_dependencies;

        {
            // Scope for delineating recursive calls
            self.visiting.borrow_mut().push(key.clone());
            *self.current_depth.borrow_mut() += 1;

            let node_modules_path = install_path.join("node_modules");
            if node_modules_path.exists() {
                let sub_resolver = node_modules.create_child(install_path.clone())?;

                resolved_dependencies = self.resolve_deps(&dependencies, &sub_resolver)?;
                resolved_dev_dependencies = self.resolve_deps(&dev_dependencies, &sub_resolver)?;
            } else {
                resolved_dependencies = self.resolve_deps(&dependencies, node_modules)?;
                resolved_dev_dependencies = self.resolve_deps(&dev_dependencies, node_modules)?;
            }

            *self.current_depth.borrow_mut() -= 1;
            self.visiting.borrow_mut().pop();
        }

        let package = Package {
            name: name.clone(),
            version: version.clone(),
            dependencies: resolved_dependencies,
            dev_dependencies: resolved_dev_dependencies,
            visited: RefCell::new(false),
            dep_resolver: Rc::downgrade(self),
            data: package_data.clone(),
        };
        self.packages
            .borrow_mut()
            .insert(key.clone(), Rc::new(package));

        Ok(PackageEntry::Resolved(key))
    }

    fn resolve_deps(
        self: &Rc<Self>,
        deps: &HashMap<String, ExtendedVersionReq>,
        node_modules: &Rc<NodeModules>,
    ) -> Result<HashMap<String, Dependency>> {
        let mut packages = HashMap::new();
        for (name, version_req) in deps {
            if let Some(data) = node_modules.get_package(name) {
                // A version of that dependency exists
                packages.insert(
                    name.clone(),
                    Dependency {
                        name: name.clone(),
                        version_req: version_req.clone(),
                        package: self.resolve_package(&data, node_modules)?,
                    },
                );
            } else {
                // No version of that dependency exists
                packages.insert(
                    name.clone(),
                    Dependency {
                        name: name.clone(),
                        version_req: version_req.clone(),
                        package: PackageEntry::Missing,
                    },
                );
            }
        }
        Ok(packages)
    }

    pub fn get_package(&self, key: &PackageKey) -> Option<Rc<Package>> {
        self.packages.borrow().get(key).map(|r| r.clone())
    }

    pub fn unwrap_entry(self: &Rc<Self>, entry: PackageEntry) -> Result<Rc<Package>> {
        match entry {
            PackageEntry::Resolved(key) => Ok(self
                .get_package(&key)
                .ok_or(eyre!("Root package is missing"))?),
            PackageEntry::Truncated => Err(eyre!("Root package is truncated")),
            PackageEntry::Missing => Err(eyre!("Root package is missing")),
        }
    }

    pub fn resolve_root_package(
        self: &Rc<Self>,
        package_data: &PackageJsonData,
    ) -> Result<Rc<Package>> {
        let entry = self.resolve_package(package_data, &self.root_node_modules)?;
        self.unwrap_entry(entry)
    }

    pub(crate) fn refresh_visited(&self) {
        self.visiting.borrow_mut().clear();
        *self.current_depth.borrow_mut() = 0;
        for package in self.packages.borrow().values() {
            *package.visited.borrow_mut() = false;
        }
    }
}
