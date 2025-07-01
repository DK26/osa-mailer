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
    use super::*;
    use std::env;
    use std::fs::{self, File};
    use std::io::Write;
    use std::path::Path;

    /// Helper to create a file at a given path for testing.
    fn create_test_file(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut file = File::create(path).unwrap();
        writeln!(file, "test").unwrap();
    }

    #[test]
    fn resolves_relative_path_to_exe_dir() {
        // Simulate a file relative to the executable
        let rel = "myconfig.toml";
        let rel_path = RelativePath::new(rel).unwrap();
        let exe_dir = current_exe().unwrap().parent().unwrap().to_path_buf();
        let expected = exe_dir.join(rel);

        assert_eq!(rel_path.full_path, expected);
        assert!(rel_path.full_path.is_absolute());
    }

    #[test]
    fn resolves_relative_path_with_custom_cwd() {
        // Use a temp directory as the cwd
        let temp_dir = env::temp_dir().join("osa_mailer_test_cwd");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let rel = "subdir/file.txt";
        let rel_path = RelativePath::new(rel).unwrap().cwd(&temp_dir);
        let expected = temp_dir.join(rel);

        assert_eq!(rel_path.full_path, expected);
        assert!(rel_path.full_path.is_absolute());
    }

    #[test]
    fn does_not_modify_absolute_path() {
        // If an absolute path is provided, cwd should not affect it
        let abs = env::temp_dir().join("absolute_file.txt");
        let rel_path = RelativePath::new(&abs).unwrap().cwd("/should/not/use");
        assert_eq!(rel_path.full_path, abs);
    }

    #[test]
    fn can_access_file_using_full_path() {
        // Actually create a file and check that the full path points to it
        let temp_dir = env::temp_dir().join("osa_mailer_test_access");
        let file_name = "access.txt";
        let file_path = temp_dir.join(file_name);
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();
        create_test_file(&file_path);

        let rel_path = RelativePath::new(file_name).unwrap().cwd(&temp_dir);
        assert!(rel_path.full_path.exists());
        assert_eq!(rel_path.full_path, file_path);
    }

    #[test]
    fn handles_dot_and_dotdot_components() {
        let temp_dir = env::temp_dir().join("osa_mailer_test_dot");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let rel = "./foo/../bar.txt";
        let rel_path = RelativePath::new(rel).unwrap().cwd(&temp_dir);
        let expected = temp_dir.join(rel);

        // Create the file so canonicalize() works
        if let Some(parent) = rel_path.full_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        File::create(&rel_path.full_path).unwrap();

        assert_eq!(rel_path.full_path, expected);
        // The path should resolve correctly even with . and ..
        assert_eq!(
            rel_path.full_path.canonicalize().unwrap().parent().unwrap(),
            temp_dir.canonicalize().unwrap()
        );
    }

    #[test]
    fn as_ref_and_into_pathbuf_are_consistent() {
        let rel = "somefile.txt";
        let rel_path = RelativePath::new(rel).unwrap();
        let as_ref_path: &Path = rel_path.as_ref();
        let into_pathbuf: PathBuf = rel_path.clone().into();
        assert_eq!(as_ref_path, into_pathbuf.as_path());
    }
}
