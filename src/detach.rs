use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};

use crate::args::Options;
use crate::config::Cache;
use crate::filesystem::{self, DetachState, Filesystem, PreparedFile};

pub fn detach(opt: &Options) -> Result<()> {
    ensure!(!opt.force, "detach does not support --force");
    ensure!(!opt.patch, "detach does not support --patch");
    filesystem::validate_detach_cache(&opt.cache_file)?;
    let mut cache: Cache =
        filesystem::load_file(&opt.cache_file)?.context("cannot detach without a cache")?;
    // No configuration, template helpers or hooks are needed to leave files in place.
    if opt.dry_run {
        run(
            &mut filesystem::DryRunFilesystem::new(),
            &mut cache,
            &opt.cache_file,
            true,
        )
    } else {
        run(
            &mut filesystem::RealFilesystem::new(opt.noconfirm),
            &mut cache,
            &opt.cache_file,
            false,
        )
    }
}

struct Entry {
    source: PathBuf,
    target: PathBuf,
    template: bool,
    state: DetachState,
    prepared: Option<PreparedFile>,
}

fn run(
    fs: &mut impl Filesystem,
    cache: &mut Cache,
    cache_file: &Path,
    dry_run: bool,
) -> Result<()> {
    let mut entries = Vec::new();
    let mut targets = BTreeSet::new();
    let mut identities = BTreeSet::new();
    let cache_identity = filesystem::file_identity(cache_file)?;
    let mut errors = false;
    for (template, files) in [(false, &cache.symlinks), (true, &cache.templates)] {
        for (source, target) in files {
            let validation = (|| {
                let absolute = target_location(target)?;
                ensure!(
                    absolute != target_location(cache_file)?,
                    "target is the cache file"
                );
                ensure!(
                    targets.insert(absolute),
                    "target occurs more than once in cache"
                );
                let expected_source = cache.detach_sources.get(source).unwrap_or(source);
                let state =
                    fs.inspect_detach((!template).then_some(expected_source.as_path()), target)?;
                if let Some(identity) = filesystem::file_identity(target)? {
                    ensure!(
                        Some(identity) != cache_identity,
                        "target aliases the cache file"
                    );
                    ensure!(
                        identities.insert(identity),
                        "target aliases another cached target"
                    );
                }
                Ok(state)
            })();
            match validation {
                Ok(state) => entries.push(Entry {
                    source: source.clone(),
                    target: target.clone(),
                    template,
                    state,
                    prepared: None,
                }),
                Err(error) => {
                    error!("Cannot detach {target:?}: {error:#}");
                    errors = true;
                }
            }
        }
    }
    ensure!(
        !errors,
        "preflight failed; no files or cache entries were changed"
    );

    for entry in &entries {
        match &entry.state {
            DetachState::Symlink(_) => info!("Detach {:?} as a regular file", entry.target),
            DetachState::File => info!("Leave {:?} unchanged and stop tracking it", entry.target),
            DetachState::Missing => warn!(
                "Target {:?} is missing; forget its cache entry",
                entry.target
            ),
        }
    }
    if dry_run {
        return Ok(());
    }

    // Prepare every copy before replacing any link. PreparedFile cleans up on error.
    for entry in &mut entries {
        if let DetachState::Symlink(source) = &entry.state {
            entry.prepared = Some(fs.prepare_detach(source, &entry.target).with_context(|| {
                format!("prepare {:?}; no targets were replaced", entry.target)
            })?);
        }
    }
    for entry in &entries {
        recheck(fs, entry)?;
    }

    // Persist every source binding before changing a link in another source's
    // chain. A failed per-file checkpoint must still be safe to retry on disk.
    let mut checkpoint = cache.clone();
    checkpoint.detach_sources = entries
        .iter()
        .filter_map(|entry| match &entry.state {
            DetachState::Symlink(source) => Some((entry.source.clone(), source.clone())),
            _ => None,
        })
        .collect();
    if checkpoint.detach_sources != cache.detach_sources {
        let mut prepared_cache = fs
            .prepare_detach_cache(cache_file, &checkpoint)
            .context("prepare source bindings before detaching any files")?;
        fs.replace_detach(&mut prepared_cache, cache_file)
            .context("save source bindings before detaching any files")?;
        *cache = checkpoint;
    }

    for entry in &mut entries {
        recheck(fs, entry)?;
        let mut checkpoint = cache.clone();
        if entry.template {
            checkpoint.templates.remove(&entry.source);
        } else {
            checkpoint.symlinks.remove(&entry.source);
            checkpoint.detach_sources.remove(&entry.source);
        }
        let mut prepared_cache = fs
            .prepare_detach_cache(cache_file, &checkpoint)
            .with_context(|| format!("prepare cache before detaching {:?}", entry.target))?;
        if let Some(prepared) = &mut entry.prepared {
            fs.replace_detach(prepared, &entry.target)
                .with_context(|| {
                    format!(
                        "replace {:?}; previous files may already be detached",
                        entry.target
                    )
                })?;
        }
        fs.replace_detach(&mut prepared_cache, cache_file)
            .with_context(|| {
                format!(
                    "save cache after detaching {:?}; rerun detach to reconcile the cache",
                    entry.target
                )
            })?;
        *cache = checkpoint;
    }
    Ok(())
}

fn recheck(fs: &mut impl Filesystem, entry: &Entry) -> Result<()> {
    let source = match &entry.state {
        DetachState::Symlink(source) => source,
        _ => &entry.source,
    };
    let state = fs
        .inspect_detach((!entry.template).then_some(source.as_path()), &entry.target)
        .with_context(|| format!("recheck {:?}", entry.target))?;
    if state != entry.state {
        bail!(
            "target {:?} changed after preflight; stop without replacing it",
            entry.target
        );
    }
    Ok(())
}

// Resolve parent aliases without following the final symlink being detached.
// Missing targets can also have missing parents; they only lose a cache entry.
fn target_location(path: &Path) -> Result<PathBuf> {
    let absolute = std::path::absolute(path).context("resolve target path")?;
    if let (Some(parent), Some(name)) = (absolute.parent(), absolute.file_name()) {
        if let Ok(parent) = parent.canonicalize() {
            return Ok(parent.join(name));
        }
    }
    Ok(absolute)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filesystem::MockFilesystem;

    fn two_links() -> Cache {
        Cache {
            symlinks: [("a".into(), "home/a".into()), ("b".into(), "home/b".into())].into(),
            templates: Default::default(),
            detach_sources: Default::default(),
        }
    }

    #[cfg(unix)]
    #[test]
    fn source_chain_can_resume_after_a_failed_checkpoint() {
        use crate::filesystem::RealFilesystem;
        use std::fs;
        use std::os::unix::fs::symlink;

        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.0).unwrap();
            }
        }

        for fail_first_checkpoint in [true, false] {
            let root = std::env::temp_dir().join(format!(
                "dotter-detach-resume-{}-{fail_first_checkpoint}",
                std::process::id()
            ));
            fs::create_dir(&root).unwrap();
            let root = Cleanup(root.canonicalize().unwrap());
            let source = root.0.join("a-source");
            let alias = root.0.join("z-source");
            let first = root.0.join("first");
            let second = root.0.join("second");
            let cache_file = root.0.join("cache.toml");
            fs::write(&source, "keep").unwrap();
            symlink(&source, &first).unwrap();
            symlink(&first, &alias).unwrap();
            symlink(&source, &second).unwrap();
            let mut cache = Cache::default();
            cache.symlinks.insert(source.clone(), first.clone());
            cache.symlinks.insert(alias, second.clone());
            filesystem::save_file(&cache_file, &cache).unwrap();

            let mut fs = MockFilesystem::new();
            fs.expect_inspect_detach().returning(|source, target| {
                RealFilesystem::new(true).inspect_detach(source, target)
            });
            fs.expect_prepare_detach().returning(|source, target| {
                RealFilesystem::new(true).prepare_detach(source, target)
            });
            fs.expect_prepare_detach_cache()
                .returning(move |path, cache| {
                    if !fail_first_checkpoint && cache.symlinks.is_empty() {
                        bail!("cache preparation failed after the first checkpoint");
                    }
                    RealFilesystem::new(true).prepare_detach_cache(path, cache)
                });
            let first_target = first.clone();
            let cache_target = cache_file.clone();
            fs.expect_replace_detach()
                .returning(move |prepared, target| {
                    if fail_first_checkpoint
                        && target == cache_target
                        && !fs::symlink_metadata(&first_target).unwrap().is_symlink()
                    {
                        bail!("first checkpoint replacement failed");
                    }
                    RealFilesystem::new(true).replace_detach(prepared, target)
                });
            assert!(run(&mut fs, &mut cache, &cache_file, false).is_err());
            assert!(!fs::symlink_metadata(&first).unwrap().is_symlink());
            assert!(fs::symlink_metadata(&second).unwrap().is_symlink());

            // Reload from disk, as a new process would after the failure.
            let mut cache = filesystem::load_file(&cache_file).unwrap().unwrap();
            let unrelated = root.0.join("unrelated");
            fs::write(&unrelated, "do not copy").unwrap();
            fs::remove_file(&second).unwrap();
            symlink(&unrelated, &second).unwrap();
            let saved_cache = fs::read(&cache_file).unwrap();
            assert!(
                run(
                    &mut RealFilesystem::new(true),
                    &mut cache,
                    &cache_file,
                    false
                )
                .is_err()
            );
            assert!(fs::symlink_metadata(&second).unwrap().is_symlink());
            assert_eq!(fs::read(&cache_file).unwrap(), saved_cache);
            fs::remove_file(&second).unwrap();
            symlink(&source, &second).unwrap();
            run(
                &mut RealFilesystem::new(true),
                &mut cache,
                &cache_file,
                false,
            )
            .unwrap();
            fs::remove_file(source).unwrap();
            assert_eq!(fs::read(first).unwrap(), b"keep");
            assert_eq!(fs::read(second).unwrap(), b"keep");
            assert!(cache.symlinks.is_empty());
            assert!(cache.detach_sources.is_empty());
            assert!(
                !fs::read_to_string(cache_file)
                    .unwrap()
                    .contains("detach_sources")
            );
        }
    }

    #[test]
    fn preparing_copies_can_fail_without_any_replacements() {
        let mut cache = two_links();
        let mut fs = MockFilesystem::new();
        fs.expect_inspect_detach()
            .times(2)
            .returning(|_, _| Ok(DetachState::Symlink(PathBuf::from("resolved-source"))));
        fs.expect_prepare_detach()
            .withf(|_, target| target == Path::new("home/a"))
            .times(1)
            .returning(|_, _| Ok(PreparedFile::default()));
        fs.expect_prepare_detach()
            .withf(|_, target| target == Path::new("home/b"))
            .times(1)
            .returning(|_, _| bail!("disk full"));
        fs.expect_replace_detach().never();
        fs.expect_prepare_detach_cache().never();
        assert!(run(&mut fs, &mut cache, Path::new("cache.toml"), false).is_err());
        assert_eq!(cache.symlinks.len(), 2);
    }

    #[test]
    fn target_changed_during_preparation_blocks_all_replacements() {
        let mut cache = two_links();
        let mut fs = MockFilesystem::new();
        let mut sequence = mockall::Sequence::new();
        fs.expect_inspect_detach()
            .times(2)
            .in_sequence(&mut sequence)
            .returning(|_, _| Ok(DetachState::Symlink(PathBuf::from("resolved-source"))));
        fs.expect_prepare_detach()
            .times(2)
            .in_sequence(&mut sequence)
            .returning(|_, _| Ok(PreparedFile::default()));
        fs.expect_inspect_detach()
            .times(1)
            .in_sequence(&mut sequence)
            .returning(|_, _| Ok(DetachState::File));
        fs.expect_replace_detach().never();
        fs.expect_prepare_detach_cache().never();
        assert!(run(&mut fs, &mut cache, Path::new("cache.toml"), false).is_err());
        assert_eq!(cache.symlinks.len(), 2);
    }

    #[test]
    fn cache_preparation_failure_leaves_targets_unchanged() {
        let mut cache = two_links();
        let mut fs = MockFilesystem::new();
        fs.expect_inspect_detach()
            .times(4)
            .returning(|_, _| Ok(DetachState::Symlink(PathBuf::from("resolved-source"))));
        fs.expect_prepare_detach()
            .times(2)
            .returning(|_, _| Ok(PreparedFile::default()));
        fs.expect_prepare_detach_cache()
            .times(1)
            .returning(|_, _| bail!("cache directory not writable"));
        fs.expect_replace_detach().never();
        assert!(run(&mut fs, &mut cache, Path::new("cache.toml"), false).is_err());
        assert_eq!(cache.symlinks.len(), 2);
    }

    #[test]
    fn checkpoint_failure_keeps_last_successful_checkpoint() {
        let mut cache = two_links();
        let mut fs = MockFilesystem::new();
        fs.expect_inspect_detach()
            .times(6)
            .returning(|_, _| Ok(DetachState::Symlink(PathBuf::from("resolved-source"))));
        fs.expect_prepare_detach()
            .times(2)
            .returning(|_, _| Ok(PreparedFile::default()));
        fs.expect_prepare_detach_cache()
            .times(3)
            .returning(|_, _| Ok(PreparedFile::default()));
        let mut checkpoints = 0;
        fs.expect_replace_detach()
            .times(5)
            .returning(move |_, path| {
                if path == Path::new("cache.toml") {
                    checkpoints += 1;
                    if checkpoints == 3 {
                        bail!("cache rename failed");
                    }
                }
                Ok(())
            });
        assert!(run(&mut fs, &mut cache, Path::new("cache.toml"), false).is_err());
        assert_eq!(cache.symlinks.len(), 1);
        assert!(cache.symlinks.contains_key(Path::new("b")));
    }

    #[test]
    fn replacement_failure_does_not_advance_cache() {
        let mut cache = two_links();
        let mut fs = MockFilesystem::new();
        fs.expect_inspect_detach()
            .times(5)
            .returning(|_, _| Ok(DetachState::Symlink(PathBuf::from("resolved-source"))));
        fs.expect_prepare_detach()
            .times(2)
            .returning(|_, _| Ok(PreparedFile::default()));
        fs.expect_prepare_detach_cache()
            .times(2)
            .returning(|_, _| Ok(PreparedFile::default()));
        fs.expect_replace_detach().times(2).returning(|_, target| {
            if target == Path::new("cache.toml") {
                Ok(())
            } else {
                bail!("rename failed")
            }
        });
        assert!(run(&mut fs, &mut cache, Path::new("cache.toml"), false).is_err());
        assert_eq!(cache.symlinks.len(), 2);
    }
}
