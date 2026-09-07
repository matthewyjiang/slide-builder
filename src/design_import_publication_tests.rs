use super::publish_staging;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use super::rename_no_replace;
use std::fs;

#[test]
fn publication_uses_the_first_free_name_without_changing_collisions() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    fs::create_dir(root.join("design")).unwrap();
    fs::write(root.join("design-2"), "existing file").unwrap();
    fs::create_dir(root.join("design-4")).unwrap();
    let staging = root.join("staging");
    fs::create_dir(&staging).unwrap();
    fs::write(staging.join("DESIGN.md"), "# Imported").unwrap();

    let published = publish_staging(&staging, root, "design").unwrap();

    assert_eq!(published, root.join("design-3"));
    assert_eq!(
        fs::read_to_string(published.join("DESIGN.md")).unwrap(),
        "# Imported"
    );
    assert!(!staging.exists());
    assert_eq!(fs::read_dir(root.join("design")).unwrap().count(), 0);
    assert_eq!(
        fs::read_to_string(root.join("design-2")).unwrap(),
        "existing file"
    );
    assert_eq!(fs::read_dir(root.join("design-4")).unwrap().count(), 0);
}

#[test]
fn publication_uses_the_last_numbered_suffix_then_a_uuid() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    fs::create_dir(root.join("design")).unwrap();
    for suffix in 2..10_000 {
        fs::create_dir(root.join(format!("design-{suffix}"))).unwrap();
    }
    for numbered in [true, false] {
        let staging = root.join("staging");
        fs::create_dir(&staging).unwrap();
        fs::write(staging.join("DESIGN.md"), "# Imported").unwrap();

        let published = publish_staging(&staging, root, "design").unwrap();

        if numbered {
            assert_eq!(published, root.join("design-10000"));
        } else {
            let name = published.file_name().unwrap().to_str().unwrap();
            uuid::Uuid::parse_str(name.strip_prefix("design-").unwrap()).unwrap();
        }
        assert_eq!(
            fs::read_to_string(published.join("DESIGN.md")).unwrap(),
            "# Imported"
        );
        assert!(!staging.exists());
    }
}

#[test]
fn publication_returns_non_collision_errors_without_moving_staging() {
    let directory = tempfile::tempdir().unwrap();
    let staging = directory.path().join("staging");
    fs::create_dir(&staging).unwrap();
    fs::write(staging.join("DESIGN.md"), "# Imported").unwrap();
    let root = directory.path().join("missing");

    let error = publish_staging(&staging, &root, "design").unwrap_err();

    assert_eq!(
        error.downcast_ref::<std::io::Error>().unwrap().kind(),
        std::io::ErrorKind::NotFound
    );
    assert!(error
        .to_string()
        .contains(&root.join("design").display().to_string()));
    assert_eq!(
        fs::read_to_string(staging.join("DESIGN.md")).unwrap(),
        "# Imported"
    );
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn publication_cannot_replace_an_empty_directory_or_dangling_symlink() {
    let directory = tempfile::tempdir().unwrap();
    let staging = directory.path().join("staging");
    let destination = directory.path().join("design");
    fs::create_dir(&staging).unwrap();
    fs::write(staging.join("DESIGN.md"), "# Imported").unwrap();
    fs::create_dir(&destination).unwrap();
    assert_eq!(
        rename_no_replace(&staging, &destination)
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::AlreadyExists
    );
    assert!(staging.join("DESIGN.md").is_file());
    assert_eq!(fs::read_dir(&destination).unwrap().count(), 0);
    fs::remove_dir(&destination).unwrap();
    std::os::unix::fs::symlink(directory.path().join("missing"), &destination).unwrap();
    assert_eq!(
        rename_no_replace(&staging, &destination)
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::AlreadyExists
    );
    assert!(fs::symlink_metadata(&destination)
        .unwrap()
        .file_type()
        .is_symlink());
    let target = directory.path().join("target");
    fs::create_dir(&target).unwrap();
    std::os::unix::fs::symlink(&target, directory.path().join("design-2")).unwrap();
    assert_eq!(
        publish_staging(&staging, directory.path(), "design").unwrap(),
        directory.path().join("design-3")
    );
    assert_eq!(
        fs::read_link(&destination).unwrap(),
        directory.path().join("missing")
    );
    assert_eq!(
        fs::read_link(directory.path().join("design-2")).unwrap(),
        target
    );
    assert_eq!(fs::read_dir(target).unwrap().count(), 0);
}

#[test]
fn competing_imports_publish_distinct_complete_packages() {
    let directory = tempfile::tempdir().unwrap();
    let barrier = std::sync::Barrier::new(2);
    std::thread::scope(|scope| {
        let imports: Vec<_> = ["first", "second"]
            .into_iter()
            .map(|name| {
                let root = directory.path();
                let barrier = &barrier;
                scope.spawn(move || {
                    let staging = root.join(name);
                    fs::create_dir(&staging).unwrap();
                    fs::write(staging.join("DESIGN.md"), name).unwrap();
                    barrier.wait();
                    let published = publish_staging(&staging, root, "design").unwrap();
                    assert_eq!(
                        fs::read_to_string(published.join("DESIGN.md")).unwrap(),
                        name
                    );
                    published
                })
            })
            .collect();
        let paths: Vec<_> = imports
            .into_iter()
            .map(|task| task.join().unwrap())
            .collect();
        assert_ne!(paths[0], paths[1]);
        let mut paths = paths;
        paths.sort();
        assert_eq!(
            paths,
            vec![
                directory.path().join("design"),
                directory.path().join("design-2")
            ]
        );
    });
}
