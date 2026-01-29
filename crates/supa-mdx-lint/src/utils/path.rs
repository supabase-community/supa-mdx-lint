use std::{
    ops::Deref,
    path::{Component, Path, PathBuf},
};

pub(crate) struct IsGlob(pub bool);

impl Deref for IsGlob {
    type Target = bool;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

fn normalize_glob_path(path: &Path) -> PathBuf {
    let components = path.components();
    let mut result = PathBuf::new();

    for component in components {
        match component {
            Component::ParentDir => {
                result.pop();
            }
            Component::CurDir => {}
            _ => {
                result.push(component);
            }
        }
    }

    result
}

/// Normalize a path for pattern comparison by canonicalizing it, and replacing
/// Windows-specific features:
/// - Replace ackslashes with slashes
/// - Remove extended-length prefix
pub(crate) fn normalize_path(path: &Path, is_glob: IsGlob) -> String {
    let path = if *is_glob {
        normalize_glob_path(path)
    } else {
        path.canonicalize().unwrap_or(path.into())
    };
    let mut path_str = path.to_string_lossy().replace("\\", "/");
    if path_str.starts_with("//?/") {
        path_str = path_str[4..].to_string();
    }
    path_str
}
