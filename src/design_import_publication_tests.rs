use super::{publish_staging, rename_no_replace};
use std::fs;

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
    assert_eq!(
        publish_staging(&staging, directory.path(), "design").unwrap(),
        directory.path().join("design-2")
    );
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
    });
}
