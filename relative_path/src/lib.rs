use std::env::current_exe;
use std::path::{Path, PathBuf};

/// If a full path was not provided, automatically produces a full path out of a relative path to the executable location.
/// e.g. `RelativePath::new("cfg.toml")` allows us to get a reference (a `&Path` from `as_ref()`)
/// which includes the full path to the home directory, joined together with the `cfg.toml` file name.
#[derive(Clone, Debug)]
pub struct RelativePath {
    relative_path: PathBuf,
    full_path: PathBuf,
}

impl RelativePath {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, std::io::Error> {
        let exe_dir = current_exe()?
            .parent()
            .unwrap() // a binary file path always has a parent
            .to_path_buf();

        Ok(Self {
            relative_path: path.as_ref().to_path_buf(),
            full_path: exe_dir.join(path),
        })
    }

    /// Sets the current working directory from which relative paths generate full paths.
    /// Note: If the relative path contains a full path, this will be ignored.
    pub fn cwd(mut self, cwd: impl AsRef<Path>) -> Self {
        self.full_path = cwd.as_ref().join(&self.relative_path);
        self
    }
}

impl std::fmt::Display for RelativePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.full_path.display())
    }
}

impl From<RelativePath> for PathBuf {
    fn from(relative_path: RelativePath) -> Self {
        relative_path.full_path
    }
}
impl AsRef<Path> for RelativePath {
    #[inline]
    fn as_ref(&self) -> &Path {
        self.full_path.as_ref()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
