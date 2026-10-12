use bhasa::{package_lock::Snapshot, project};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static SERIAL: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "bhasa-lock-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("app/src")).unwrap();
        fs::create_dir_all(path.join("math/src")).unwrap();
        fs::write(path.join("app/bhasa.toml"), "[package]\nname = \"app\"\nversion = \"0.1.0\"\nentry = \"src/main.bh\"\n[dependencies]\nmath = \"../math\"\n").unwrap();
        fs::write(
            path.join("math/bhasa.toml"),
            "[package]\nname = \"math\"\nversion = \"1.2.3\"\nentry = \"src/lib.bh\"\n",
        )
        .unwrap();
        fs::write(
            path.join("app/src/main.bh"),
            "import \"@math\"\ndef main() { print(double(21)) }\n",
        )
        .unwrap();
        fs::write(
            path.join("math/src/lib.bh"),
            "def double(value: int) -> int { return value * 2 }\n",
        )
        .unwrap();
        Self(path)
    }
    fn snapshot(&self) -> Snapshot {
        let loaded = project::load_unparsed(Some(&self.0.join("app/src/main.bh"))).unwrap();
        let root = project::read_manifest(&self.0.join("app/bhasa.toml")).unwrap();
        Snapshot::capture(&loaded, &root).unwrap()
    }
    fn lock(&self) -> String {
        let lock = self.snapshot().encode();
        fs::write(self.0.join("app/bhasa.lock"), &lock).unwrap();
        lock
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn loaded_graph_is_relocatable_and_unused_build_files_do_not_change_it() {
    let first = Fixture::new();
    let second = Fixture::new();
    assert_eq!(first.snapshot(), second.snapshot());
    let encoded = first.lock();
    fs::create_dir_all(first.0.join("app/build/rust/cache")).unwrap();
    fs::write(
        first.0.join("app/build/rust/cache/output"),
        b"ignored artifact",
    )
    .unwrap();
    fs::write(first.0.join("math/src/unused.bh"), b"unused").unwrap();
    assert_eq!(first.snapshot().encode(), encoded);
    assert!(project::load(Some(&first.0.join("app/src/main.bh"))).is_ok());
}

#[test]
fn checksum_failures_preserve_lock_and_report_its_path_in_both_loader_modes() {
    let fixture = Fixture::new();
    let lock = fixture.lock();
    fs::write(
        fixture.0.join("math/src/lib.bh"),
        "def double(value: int) -> int { return value * 3 }\n",
    )
    .unwrap();
    for parsed in [false, true] {
        let path = fixture.0.join("app/src/main.bh");
        let errors = if parsed {
            project::load(Some(&path))
        } else {
            project::load_unparsed(Some(&path))
        }
        .unwrap_err();
        assert!(errors[0].diagnostic.message.contains("checksum mismatch"));
        assert_eq!(
            errors[0].path,
            fixture.0.join("app/bhasa.lock").canonicalize().unwrap()
        );
    }
    assert_eq!(
        fs::read_to_string(fixture.0.join("app/bhasa.lock")).unwrap(),
        lock
    );
}

#[test]
fn exact_manifest_bytes_and_import_membership_are_locked() {
    let fixture = Fixture::new();
    fixture.lock();
    let manifest = fixture.0.join("math/bhasa.toml");
    let old = fs::read_to_string(&manifest).unwrap();
    fs::write(&manifest, format!("{old}# edited\n")).unwrap();
    assert!(
        project::load_unparsed(Some(&fixture.0.join("app/src/main.bh"))).unwrap_err()[0]
            .diagnostic
            .message
            .contains("checksum mismatch")
    );
    fs::write(&manifest, old).unwrap();
    fs::write(fixture.0.join("app/src/main.bh"), "def main() {}\n").unwrap();
    assert!(project::load_unparsed(Some(&fixture.0.join("app/src/main.bh"))).is_err());
}

#[test]
fn capture_uses_loaded_source_bytes_even_if_disk_changes_later() {
    let fixture = Fixture::new();
    let loaded = project::load_unparsed(Some(&fixture.0.join("app/src/main.bh"))).unwrap();
    let root = loaded.manifests.iter().find(|m| m.name == "app").unwrap();
    let before = Snapshot::capture(&loaded, root).unwrap();
    fs::write(fixture.0.join("math/src/lib.bh"), "changed after loading").unwrap();
    fs::write(fixture.0.join("math/bhasa.toml"), "changed after loading").unwrap();
    assert_eq!(Snapshot::capture(&loaded, root).unwrap(), before);
}

#[test]
fn malformed_oversized_and_nonregular_locks_fail_closed() {
    let fixture = Fixture::new();
    let path = fixture.0.join("app/bhasa.lock");
    for contents in ["{}".to_owned(), "x".repeat(1024 * 1024 + 1)] {
        fs::write(&path, contents).unwrap();
        assert!(project::load_unparsed(Some(&fixture.0.join("app/src/main.bh"))).is_err());
    }
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(project::load_unparsed(Some(&fixture.0.join("app/src/main.bh"))).is_err());
}

#[test]
fn explicit_lock_update_loads_changes_while_normal_loading_stays_strict() {
    let fixture = Fixture::new();
    fixture.lock();
    fs::write(
        fixture.0.join("math/src/lib.bh"),
        "def double(value: int) -> int { return value * 4 }\n",
    )
    .unwrap();
    let entry = fixture.0.join("app/src/main.bh");
    assert!(project::load_unparsed(Some(&entry)).is_err());
    let updated = project::load_unparsed_for_lock_update(Some(&entry)).unwrap();
    let root = updated.manifests.iter().find(|m| m.name == "app").unwrap();
    let snapshot = Snapshot::capture(&updated, root).unwrap();
    fs::write(fixture.0.join("app/bhasa.lock"), snapshot.encode()).unwrap();
    assert!(project::load_unparsed(Some(&entry)).is_ok());
}

#[test]
fn transitive_packages_are_checked_by_path_even_when_names_and_versions_match() {
    let fixture = Fixture::new();
    fs::create_dir_all(fixture.0.join("leaf/src")).unwrap();
    fs::write(
        fixture.0.join("leaf/bhasa.toml"),
        "[package]\nname = \"math\"\nversion = \"1.2.3\"\nentry = \"src/lib.bh\"\n",
    )
    .unwrap();
    fs::write(
        fixture.0.join("leaf/src/lib.bh"),
        "def leaf(value: int) -> int { return value }\n",
    )
    .unwrap();
    let manifest = fixture.0.join("math/bhasa.toml");
    let old = fs::read_to_string(&manifest).unwrap();
    fs::write(
        &manifest,
        format!("{old}[dependencies]\nleaf = \"../leaf\"\n"),
    )
    .unwrap();
    fs::write(
        fixture.0.join("math/src/lib.bh"),
        "import \"@leaf\"\ndef double(value: int) -> int { return leaf(value) * 2 }\n",
    )
    .unwrap();
    let encoded = fixture.lock();
    assert!(encoded.contains("../leaf/src/lib.bh"));
    fs::write(
        fixture.0.join("leaf/src/lib.bh"),
        "def leaf(value: int) -> int { return value + 1 }\n",
    )
    .unwrap();
    let errors = project::load_unparsed(Some(&fixture.0.join("app/src/main.bh"))).unwrap_err();
    assert!(errors[0].diagnostic.message.contains("../leaf/src/lib.bh"));
}

#[cfg(unix)]
#[test]
fn lock_symlinks_are_rejected_even_when_the_target_is_valid() {
    let fixture = Fixture::new();
    let encoded = fixture.snapshot().encode();
    fs::write(fixture.0.join("valid.lock"), encoded).unwrap();
    std::os::unix::fs::symlink(
        fixture.0.join("valid.lock"),
        fixture.0.join("app/bhasa.lock"),
    )
    .unwrap();
    assert!(
        project::load_unparsed(Some(&fixture.0.join("app/src/main.bh"))).unwrap_err()[0]
            .diagnostic
            .message
            .contains("symlinks")
    );
}

#[test]
fn checked_test_policy_verifies_primary_graph_and_all_non_test_dependencies() {
    let fixture = Fixture::new();
    fixture.lock();
    fs::create_dir_all(fixture.0.join("app/tests")).unwrap();
    let entry = fixture.0.join("app/tests/basic.bh");
    fs::write(
        &entry,
        "import \"@math\"\ndef test_double() -> bool { return double(21) == 42 }\n",
    )
    .unwrap();
    // Normal loaders keep their exact-entry contract. Only the test loader
    // permits additional canonical sources under the entry package's tests/.
    assert!(project::load_unparsed(Some(&entry)).is_err());
    assert!(project::load_test_graph(&entry).is_ok());
    fs::write(
        &entry,
        "import \"@math\"\ndef test_double() -> bool { return double(20) == 40 }\n",
    )
    .unwrap();
    assert!(project::load_test_graph(&entry).is_ok());
    // An extra production source outside tests is not implicitly trusted.
    fs::write(fixture.0.join("app/src/unlocked.bh"), "def extra() {}\n").unwrap();
    fs::write(
        &entry,
        "import \"../src/unlocked.bh\"\ndef test_ok() -> bool { return true }\n",
    )
    .unwrap();
    assert!(project::load_test_graph(&entry).unwrap_err()[0]
        .diagnostic
        .message
        .contains("absent or changed"));
    // Even a test omitting math cannot bypass a changed primary dependency.
    fs::write(&entry, "def test_ok() -> bool { return true }\n").unwrap();
    fs::write(
        fixture.0.join("math/src/lib.bh"),
        "def double(value: int) -> int { return value * 3 }\n",
    )
    .unwrap();
    assert!(project::load_test_graph(&entry).unwrap_err()[0]
        .diagnostic
        .message
        .contains("checksum mismatch"));
}

#[cfg(unix)]
#[test]
fn canonical_manifest_entry_alias_roundtrips_and_agrees_with_explicit_loading() {
    let fixture = Fixture::new();
    let path = fixture.0.join("app/src/main.bh");
    fs::rename(&path, fixture.0.join("app/src/real.bh")).unwrap();
    std::os::unix::fs::symlink("real.bh", &path).unwrap();
    let loaded = project::load_unparsed(Some(&path)).unwrap();
    let root = loaded.manifests.iter().find(|m| m.name == "app").unwrap();
    let snapshot = Snapshot::capture(&loaded, root).unwrap();
    assert_eq!(loaded.entry, path.canonicalize().unwrap());
    assert_eq!(Snapshot::decode(&snapshot.encode()).unwrap(), snapshot);
    fs::write(fixture.0.join("app/bhasa.lock"), snapshot.encode()).unwrap();
    assert!(project::load_unparsed(Some(&path)).is_ok());
}
