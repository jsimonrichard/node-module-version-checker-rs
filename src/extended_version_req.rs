use semver::{Version, VersionReq};
use std::fmt;

#[derive(Debug, Clone)]
pub enum ExtendedVersionReq {
    SemVer(VersionReq),
    Or(Vec<ExtendedVersionReq>),
    Workspace(String),
    Unchecked(String),
}

impl ExtendedVersionReq {
    pub fn parse(version_str: &str) -> Self {
        if let Ok(semver_req) = VersionReq::parse(version_str) {
            return Self::SemVer(semver_req);
        } else if version_str.starts_with("workspace:") {
            return Self::Workspace(version_str[10..].to_string());
        } else if version_str.contains(" || ") {
            let version_reqs = version_str
                .split(" || ")
                .map(|version_str| Self::parse(version_str))
                .collect::<Vec<_>>();
            return Self::Or(version_reqs);
        } else {
            return Self::Unchecked(version_str.to_string());
        }
    }

    pub fn matches(&self, version: &Version) -> Option<bool> {
        match self {
            Self::SemVer(version_req) => Some(version_req.matches(version)),
            Self::Or(version_reqs) => Some(
                version_reqs
                    .iter()
                    .filter_map(|version_req| version_req.matches(version))
                    .any(|matches| matches),
            ),
            _ => None,
        }
    }
}

impl PartialEq for ExtendedVersionReq {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::SemVer(a), Self::SemVer(b)) => a.to_string() == b.to_string(),
            (Self::Unchecked(a), Self::Unchecked(b)) => a == b,
            (Self::Or(a), Self::Or(b)) => a.iter().all(|a| b.iter().any(|b| a.eq(b))),
            _ => false,
        }
    }
}

impl fmt::Display for ExtendedVersionReq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SemVer(req) => write!(f, "{}", req),
            Self::Or(version_reqs) => write!(
                f,
                "{}",
                version_reqs
                    .iter()
                    .map(|version_req| version_req.to_string())
                    .collect::<Vec<_>>()
                    .join(" || ")
            ),
            Self::Workspace(path) => write!(f, "workspace:{}", path),
            Self::Unchecked(version_str) => write!(f, "{}", version_str),
        }
    }
}
