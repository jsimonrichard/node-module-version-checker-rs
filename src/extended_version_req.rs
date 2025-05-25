use std::path::PathBuf;

use color_eyre::eyre::{Result, WrapErr, eyre};
use semver::{Version, VersionReq};
use std::fmt;
use tracing::instrument;

#[derive(Debug, Clone)]
pub enum ExtendedVersionReq {
    SemVer(VersionReq),
    Workspace(String),
    Path(PathBuf),
}

impl ExtendedVersionReq {
    #[instrument]
    pub fn parse(version_str: &str) -> Result<Self> {
        if version_str.starts_with("workspace:") {
            Ok(Self::Workspace(version_str[10..].to_string()))
        } else if version_str.starts_with("path:") {
            Ok(Self::Path(PathBuf::from(&version_str[5..])))
        } else if version_str.starts_with("file:") {
            Ok(Self::Path(PathBuf::from(&version_str[5..])))
        } else {
            let path = PathBuf::from(version_str);
            if path.exists() {
                return Ok(Self::Path(path));
            }
            return Ok(Self::SemVer(VersionReq::parse(version_str).wrap_err(
                eyre!("Failed to parse version requirement (or perhaps it's a path that doesn't exist): {}", version_str),
            )?));
        }
    }

    #[instrument]
    pub fn matches(&self, version: &Version) -> bool {
        match self {
            Self::SemVer(version_req) => version_req.matches(version),
            _ => true,
        }
    }
}

impl PartialEq for ExtendedVersionReq {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::SemVer(a), Self::SemVer(b)) => a.to_string() == b.to_string(),
            (Self::Workspace(a), Self::Workspace(b)) => a == b,
            (Self::Path(a), Self::Path(b)) => a == b,
            _ => false,
        }
    }
}

impl fmt::Display for ExtendedVersionReq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SemVer(req) => write!(f, "{}", req),
            Self::Workspace(path) => write!(f, "workspace:{}", path),
            Self::Path(path) => write!(f, "{}", path.display()),
        }
    }
}
