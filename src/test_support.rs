// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, MutexGuard};

static CURRENT_DIR_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

pub(crate) struct CurrentDirGuard {
    _lock: MutexGuard<'static, ()>,
    previous_dir: PathBuf,
}

impl CurrentDirGuard {
    pub(crate) fn change_to(path: &Path) -> Self {
        let lock = CURRENT_DIR_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let previous_dir = std::env::current_dir().expect("current dir");
        std::env::set_current_dir(path).expect("set cwd");

        Self {
            _lock: lock,
            previous_dir,
        }
    }
}

impl Drop for CurrentDirGuard {
    fn drop(&mut self) {
        std::env::set_current_dir(&self.previous_dir).expect("restore cwd");
    }
}
