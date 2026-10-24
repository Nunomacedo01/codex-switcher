//! Thread-local filesystem isolation for current-thread async command tests.

use anyhow::{Context, Result};
use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;

thread_local! {
    static TEST_HOME: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

pub(crate) fn home_dir() -> Result<PathBuf> {
    TEST_HOME
        .with(|home| home.borrow().clone())
        .context("TestHome must be installed before filesystem access")
}

pub(crate) struct TestHome {
    root: PathBuf,
    owns_directory: bool,
}

impl TestHome {
    pub(crate) fn new() -> Self {
        assert!(home_dir().is_err(), "test homes must not be nested");
        let root =
            std::env::temp_dir().join(format!("codex-switcher-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        TEST_HOME.with(|home| *home.borrow_mut() = Some(root.clone()));
        Self {
            root,
            owns_directory: true,
        }
    }

    pub(crate) fn path(&self) -> PathBuf {
        self.root.clone()
    }

    pub(crate) fn enter(root: PathBuf) -> Self {
        assert!(home_dir().is_err(), "test homes must not be nested");
        TEST_HOME.with(|home| *home.borrow_mut() = Some(root.clone()));
        Self {
            root,
            owns_directory: false,
        }
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        TEST_HOME.with(|home| *home.borrow_mut() = None);
        // Only remove the unique temporary directory created by this fixture.
        if self.owns_directory {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
