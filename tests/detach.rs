use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "dotter-detach-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("home")).unwrap();
        Self(root.canonicalize().unwrap())
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn cache(&self, links: &[(&Path, &Path)], templates: &[(&Path, &Path)]) {
        let mut text = String::new();
        for (name, entries) in [("symlinks", links), ("templates", templates)] {
            text.push_str(&format!("[{name}]\n"));
            for (source, target) in entries {
                text.push_str(&format!(
                    "{:?} = {:?}\n",
                    source.to_str().unwrap(),
                    target.to_str().unwrap()
                ));
            }
        }
        fs::write(self.path("cache.toml"), text).unwrap();
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_dotter"))
            .current_dir(&self.0)
            .arg("--cache-file")
            .arg(self.path("cache.toml"))
            .arg("detach")
            .args(args)
            .output()
            .unwrap()
    }

    fn no_temporary_files(&self) {
        for dir in [&self.0, &self.path("home")] {
            assert!(fs::read_dir(dir).unwrap().all(|entry| {
                !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".dotter-detach-")
            }));
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn link(source: &Path, target: &Path) -> bool {
    #[cfg(unix)]
    let result = std::os::unix::fs::symlink(source, target);
    #[cfg(windows)]
    let result = std::os::windows::fs::symlink_file(source, target);
    if let Err(error) = result {
        #[cfg(windows)]
        if error.kind() == std::io::ErrorKind::PermissionDenied {
            eprintln!("Skipping real symlink test: Windows symlink privilege unavailable");
            return false;
        }
        panic!("create test symlink: {error}");
    }
    true
}

fn success(output: Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn copies_binary_file_and_preserves_source() {
    let f = Fixture::new();
    let source = f.path("source");
    let target = f.path("home/config");
    let bytes = b"\0\xff\xfeconfig\n";
    fs::write(&source, bytes).unwrap();
    if !link(&source, &target) {
        return;
    }
    f.cache(&[(&source, &target)], &[]);
    success(f.run(&[]));
    assert!(!fs::symlink_metadata(&target).unwrap().is_symlink());
    assert_eq!(fs::read(&target).unwrap(), bytes);
    assert_eq!(fs::read(&source).unwrap(), bytes);
    fs::remove_file(&source).unwrap();
    assert_eq!(fs::read(&target).unwrap(), bytes);
    assert!(
        !fs::read_to_string(f.path("cache.toml"))
            .unwrap()
            .contains("source")
    );
    f.no_temporary_files();
    success(f.run(&[]));
}

#[test]
fn directory_blocks_entire_preflight() {
    let f = Fixture::new();
    let source = f.path("a-file");
    let target = f.path("home/file");
    fs::write(&source, "keep").unwrap();
    if !link(&source, &target) {
        return;
    }
    let directory = f.path("z-directory");
    fs::create_dir(&directory).unwrap();
    // A directory at a cached target is also unsupported on Windows.
    let dir_target = f.path("home/directory");
    fs::create_dir(&dir_target).unwrap();
    f.cache(&[(&source, &target), (&directory, &dir_target)], &[]);
    let cache = fs::read(f.path("cache.toml")).unwrap();
    assert!(!f.run(&[]).status.success());
    assert!(fs::symlink_metadata(&target).unwrap().is_symlink());
    assert_eq!(fs::read(f.path("cache.toml")).unwrap(), cache);
    f.no_temporary_files();
}

#[test]
fn changed_or_broken_link_is_not_replaced() {
    for broken in [false, true] {
        let f = Fixture::new();
        let source = f.path("source");
        let other = f.path("other");
        let target = f.path("home/config");
        fs::write(&source, "expected").unwrap();
        if !broken {
            fs::write(&other, "different").unwrap();
        }
        if !link(&other, &target) {
            return;
        }
        f.cache(&[(&source, &target)], &[]);
        let cache = fs::read(f.path("cache.toml")).unwrap();
        assert!(!f.run(&[]).status.success());
        assert_eq!(fs::read_link(&target).unwrap(), other);
        assert_eq!(fs::read(f.path("cache.toml")).unwrap(), cache);
        f.no_temporary_files();
    }
}

#[test]
fn leaves_modified_template_and_template_cache_untouched() {
    let f = Fixture::new();
    let source = f.path("nonexistent-template");
    let target = f.path("home/config");
    fs::write(&target, b"manually modified\0\xff").unwrap();
    fs::create_dir(f.path(".dotter")).unwrap();
    fs::write(f.path(".dotter/rendered"), "cached content").unwrap();
    fs::write(f.path(".dotter/pre_undeploy.sh"), "touch hook-ran").unwrap();
    f.cache(&[], &[(&source, &target)]);
    let before = fs::read(&target).unwrap();
    success(f.run(&[]));
    assert_eq!(fs::read(&target).unwrap(), before);
    assert_eq!(
        fs::read_to_string(f.path(".dotter/rendered")).unwrap(),
        "cached content"
    );
    assert!(!f.path("hook-ran").exists());
}

#[test]
fn regular_and_missing_targets_are_already_detached() {
    let f = Fixture::new();
    let source = f.path("missing-source");
    let target = f.path("home/config");
    let missing = f.path("home/missing");
    fs::write(&target, "local edits").unwrap();
    f.cache(
        &[(&source, &target)],
        &[(&f.path("other-source"), &missing)],
    );
    success(f.run(&[]));
    assert_eq!(fs::read_to_string(target).unwrap(), "local edits");
    assert!(!missing.exists());
    assert!(
        !fs::read_to_string(f.path("cache.toml"))
            .unwrap()
            .contains("missing")
    );
}

#[test]
fn dry_run_and_unsupported_flags_never_write() {
    let f = Fixture::new();
    let source = f.path("source");
    let target = f.path("home/config");
    fs::write(&source, "keep").unwrap();
    if !link(&source, &target) {
        return;
    }
    f.cache(&[(&source, &target)], &[]);
    let cache = fs::read(f.path("cache.toml")).unwrap();
    success(f.run(&["--dry-run"]));
    for args in [
        vec!["--force"],
        vec!["--force", "--dry-run"],
        vec!["--patch"],
    ] {
        assert!(!f.run(&args).status.success());
    }
    assert!(fs::symlink_metadata(&target).unwrap().is_symlink());
    assert_eq!(fs::read(f.path("cache.toml")).unwrap(), cache);
    f.no_temporary_files();
}

#[test]
fn invalid_cache_fails_without_creating_config() {
    let f = Fixture::new();
    assert!(!f.run(&[]).status.success());
    assert!(!f.path(".dotter").exists());
    fs::write(f.path("cache.toml"), "not valid toml").unwrap();
    assert!(!f.run(&[]).status.success());
    assert_eq!(
        fs::read_to_string(f.path("cache.toml")).unwrap(),
        "not valid toml"
    );
}

#[test]
fn alias_of_cache_file_cannot_be_detached() {
    let f = Fixture::new();
    let source = f.path("source");
    let target = f.path("home/../cache.toml");
    f.cache(&[(&source, &target)], &[]);
    let cache = fs::read(f.path("cache.toml")).unwrap();
    assert!(!f.run(&[]).status.success());
    assert_eq!(fs::read(f.path("cache.toml")).unwrap(), cache);
    f.no_temporary_files();
}

#[test]
fn case_alias_of_cache_file_is_rejected_on_case_insensitive_filesystems() {
    let f = Fixture::new();
    let source = f.path("source");
    let target = f.path("CACHE.TOML");
    f.cache(&[(&source, &target)], &[]);
    if !target.exists() {
        return;
    }
    let cache = fs::read(f.path("cache.toml")).unwrap();
    assert!(!f.run(&[]).status.success());
    assert_eq!(fs::read(f.path("cache.toml")).unwrap(), cache);
}

#[test]
fn duplicate_link_case_alias_blocks_preflight() {
    let f = Fixture::new();
    let source = f.path("a-source");
    let source_alias = f.path("z-source");
    let target = f.path("home/config");
    let target_alias = f.path("home/CONFIG");
    fs::write(&source, "keep").unwrap();
    if !link(&source, &target) || !link(&source, &source_alias) {
        return;
    }
    if !target_alias.exists() {
        return;
    }
    f.cache(&[(&source, &target), (&source_alias, &target_alias)], &[]);
    let cache = fs::read(f.path("cache.toml")).unwrap();
    assert!(!f.run(&[]).status.success());
    assert!(fs::symlink_metadata(target).unwrap().is_symlink());
    assert_eq!(fs::read(f.path("cache.toml")).unwrap(), cache);
}

#[test]
fn source_snapshot_survives_detaching_another_link_in_its_chain() {
    let f = Fixture::new();
    let source = f.path("a-source");
    let source_alias = f.path("z-source");
    let first = f.path("home/first");
    let second = f.path("home/second");
    fs::write(&source, "keep").unwrap();
    if !link(&source, &first) || !link(&first, &source_alias) || !link(&source, &second) {
        return;
    }
    f.cache(&[(&source, &first), (&source_alias, &second)], &[]);
    success(f.run(&[]));
    assert!(!fs::symlink_metadata(&first).unwrap().is_symlink());
    assert!(!fs::symlink_metadata(&second).unwrap().is_symlink());
    fs::remove_file(source).unwrap();
    assert_eq!(fs::read(first).unwrap(), b"keep");
    assert_eq!(fs::read(second).unwrap(), b"keep");
    success(f.run(&[]));
}

#[test]
fn resumes_source_chain_using_bindings_saved_in_cache() {
    let f = Fixture::new();
    let source = f.path("source");
    let alias = f.path("alias");
    let first = f.path("home/first");
    let second = f.path("home/second");
    fs::write(&source, "keep").unwrap();
    // The first target was already detached and checkpointed by a previous run.
    fs::write(&first, "keep").unwrap();
    if !link(&first, &alias) || !link(&source, &second) {
        return;
    }
    f.cache(&[(&alias, &second)], &[]);
    let mut cache = fs::read_to_string(f.path("cache.toml")).unwrap();
    cache.push_str(&format!(
        "[detach_sources]\n{:?} = {:?}\n",
        alias.to_str().unwrap(),
        source.to_str().unwrap()
    ));
    fs::write(f.path("cache.toml"), &cache).unwrap();
    success(f.run(&["--dry-run"]));
    assert_eq!(fs::read_to_string(f.path("cache.toml")).unwrap(), cache);
    success(f.run(&[]));
    fs::remove_file(source).unwrap();
    assert!(!fs::symlink_metadata(&second).unwrap().is_symlink());
    assert_eq!(fs::read(second).unwrap(), b"keep");
    f.no_temporary_files();
}

#[test]
fn template_symlink_and_duplicate_target_block_preflight() {
    let f = Fixture::new();
    let source = f.path("source");
    let target = f.path("home/config");
    fs::write(&source, "keep").unwrap();
    if !link(&source, &target) {
        return;
    }
    f.cache(&[], &[(&source, &target)]);
    assert!(!f.run(&[]).status.success());
    assert!(fs::symlink_metadata(&target).unwrap().is_symlink());
    f.cache(&[(&source, &target)], &[(&f.path("template"), &target)]);
    assert!(!f.run(&[]).status.success());
    assert!(fs::symlink_metadata(&target).unwrap().is_symlink());
    f.no_temporary_files();
}

#[cfg(unix)]
#[test]
fn staging_failure_preserves_every_link_and_removes_staged_files() {
    use std::os::unix::fs::PermissionsExt;
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let f = Fixture::new();
    let first = f.path("a-source");
    let second = f.path("z-source");
    let first_target = f.path("home/first");
    let locked = f.path("locked");
    fs::create_dir(&locked).unwrap();
    let second_target = locked.join("second");
    fs::write(&first, "first").unwrap();
    fs::write(&second, "second").unwrap();
    assert!(link(&first, &first_target));
    assert!(link(&second, &second_target));
    f.cache(&[(&first, &first_target), (&second, &second_target)], &[]);
    let cache = fs::read(f.path("cache.toml")).unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();
    let output = f.run(&[]);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(!output.status.success());
    assert!(fs::symlink_metadata(first_target).unwrap().is_symlink());
    assert!(fs::symlink_metadata(second_target).unwrap().is_symlink());
    assert_eq!(fs::read(f.path("cache.toml")).unwrap(), cache);
    f.no_temporary_files();
}

#[cfg(unix)]
#[test]
fn preserves_private_and_executable_modes() {
    use std::os::unix::fs::PermissionsExt;
    for mode in [0o600, 0o755] {
        let f = Fixture::new();
        let source = f.path("source");
        let target = f.path("home/config");
        fs::write(&source, "content").unwrap();
        fs::set_permissions(&source, fs::Permissions::from_mode(mode)).unwrap();
        if !link(&source, &target) {
            return;
        }
        f.cache(&[(&source, &target)], &[]);
        success(f.run(&[]));
        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            mode
        );
    }
}
