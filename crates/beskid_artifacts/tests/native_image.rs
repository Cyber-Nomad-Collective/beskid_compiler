use beskid_artifacts::native_image::{NativeImageError, PinnedNativeImages};
use sha2::{Digest, Sha256};
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[test]
fn changed_image_is_rejected_before_loading_provider() {
    let root = tempfile::tempdir().unwrap();
    let provider = root.path().join("provider");
    let image = root.path().join("image");
    std::fs::write(&provider, b"not a library").unwrap();
    std::fs::write(&image, b"changed image").unwrap();
    let error =
        unsafe { PinnedNativeImages::load(&provider, &hash(b"not a library"), &image, &hash(b"original image")) }
            .err()
            .expect("changed image cannot load");
    assert!(matches!(&error, NativeImageError::Integrity { path, .. } if path == &image.canonicalize().unwrap()));
}

#[cfg(unix)]
#[test]
fn image_symlink_is_rejected_before_loading_provider() {
    let root = tempfile::tempdir().unwrap();
    let provider = root.path().join("provider");
    let outside = root.path().join("outside");
    let image = root.path().join("image");
    std::fs::write(&provider, b"provider").unwrap();
    std::fs::write(&outside, b"outside").unwrap();
    std::os::unix::fs::symlink(&outside, &image).unwrap();
    let error = unsafe { PinnedNativeImages::load(&provider, &hash(b"provider"), &image, &hash(b"outside")) }
        .err()
        .expect("symlink cannot qualify");
    assert!(error.to_string().contains("symlink"));
}

#[cfg(unix)]
#[test]
fn provider_is_pinned_before_dependent_image_and_integrity_rechecked_for_symbols() {
    use beskid_execution::NativeExecutionControl;
    use std::{
        process::Command,
        sync::Arc,
        time::{Duration, Instant},
    };
    let root = tempfile::tempdir().unwrap();
    let provider_source = root.path().join("provider.c");
    let image_source = root.path().join("image.c");
    let provider = root.path().join(format!("provider{}", std::env::consts::DLL_SUFFIX));
    let image = root.path().join(format!("image{}", std::env::consts::DLL_SUFFIX));
    std::fs::write(&provider_source, b"int beskid_fixture_provider(void) { return 41; }").unwrap();
    std::fs::write(&image_source, b"extern int beskid_fixture_provider(void); int beskid_fixture_image(void) { return beskid_fixture_provider() + 1; }").unwrap();
    let control = NativeExecutionControl::new(Instant::now() + Duration::from_secs(30), Arc::new(|| false));
    for (source, output) in [(&provider_source, &provider), (&image_source, &image)] {
        let mut command = Command::new("cc");
        if cfg!(target_os = "macos") {
            command.args(["-dynamiclib", "-Wl,-undefined,dynamic_lookup"]);
        } else {
            command.args(["-shared", "-fPIC"]);
        }
        command.arg(source).arg("-o").arg(output);
        let result = control.run_command(&mut command, root.path(), "native image fixture").unwrap();
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    }
    let provider_hash = hash(&std::fs::read(&provider).unwrap());
    let image_bytes = std::fs::read(&image).unwrap();
    let images = unsafe { PinnedNativeImages::load(&provider, &provider_hash, &image, &hash(&image_bytes)) }.unwrap();
    let entry = unsafe { images.symbol::<unsafe extern "C" fn() -> i32>(c"beskid_fixture_image") }.unwrap();
    assert_eq!(unsafe { entry() }, 42);
    drop(entry);
    let provider_entry =
        unsafe { images.provider_symbol::<unsafe extern "C" fn() -> i32>(c"beskid_fixture_provider") }.unwrap();
    assert_eq!(unsafe { provider_entry() }, 41);
    drop(provider_entry);
    assert!(
        unsafe { images.symbol::<unsafe extern "C" fn() -> i32>(c"beskid_fixture_provider") }.is_err(),
        "an image lookup cannot admit a dependency-owned symbol"
    );
    use std::io::Write;
    std::fs::OpenOptions::new().append(true).open(&image).unwrap().write_all(b"changed").unwrap();
    assert!(images.verify_integrity().is_err());
    assert!(unsafe { images.symbol::<unsafe extern "C" fn() -> i32>(c"beskid_fixture_image") }.is_err());
}

#[cfg(unix)]
#[test]
fn two_attached_native_images_use_one_retained_provider_instance() {
    use beskid_execution::NativeExecutionControl;
    use std::{
        process::Command,
        sync::Arc,
        time::{Duration, Instant},
    };
    let root = tempfile::tempdir().unwrap();
    let provider = root.path().join(format!("attach_provider{}", std::env::consts::DLL_SUFFIX));
    let first = root.path().join(format!("attach_first{}", std::env::consts::DLL_SUFFIX));
    let second = root.path().join(format!("attach_second{}", std::env::consts::DLL_SUFFIX));
    let control = NativeExecutionControl::new(Instant::now() + Duration::from_secs(30), Arc::new(|| false));
    for (name, code, output) in [
        ("provider", "static int value=0; int attachment_fixture_next(void) { return ++value; }", &provider),
        (
            "first",
            "extern int attachment_fixture_next(void); int attachment_fixture_first(void) { return attachment_fixture_next(); }",
            &first,
        ),
        (
            "second",
            "extern int attachment_fixture_next(void); int attachment_fixture_second(void) { return attachment_fixture_next(); }",
            &second,
        ),
    ] {
        let source = root.path().join(format!("{name}.c"));
        std::fs::write(&source, code).unwrap();
        let mut command = Command::new("cc");
        if cfg!(target_os = "macos") {
            command.args(["-dynamiclib", "-Wl,-undefined,dynamic_lookup"]);
        } else {
            command.args(["-shared", "-fPIC"]);
        }
        command.arg(source).arg("-o").arg(output);
        let output = control.run_command(&mut command, root.path(), "native attachment fixture").unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    }
    let provider_hash = hash(&std::fs::read(&provider).unwrap());
    let mut images =
        unsafe { PinnedNativeImages::load(&provider, &provider_hash, &first, &hash(&std::fs::read(&first).unwrap())) }
            .unwrap();
    {
        let entry =
            unsafe { images.image_symbol::<unsafe extern "C" fn() -> i32>(0, c"attachment_fixture_first") }.unwrap();
        assert_eq!(unsafe { entry() }, 1);
    }
    let index = unsafe { images.attach(&second, &hash(&std::fs::read(&second).unwrap())) }.unwrap();
    assert_eq!(index, 1);
    assert_eq!(images.image_path_at(index).unwrap(), second.canonicalize().unwrap());
    {
        let entry =
            unsafe { images.image_symbol::<unsafe extern "C" fn() -> i32>(index, c"attachment_fixture_second") }
                .unwrap();
        assert_eq!(unsafe { entry() }, 2, "attachment must not reopen a new provider instance");
        let provider_entry =
            unsafe { images.provider_symbol::<unsafe extern "C" fn() -> i32>(c"attachment_fixture_next") }.unwrap();
        assert_eq!(unsafe { provider_entry() }, 3);
    }
    assert!(
        unsafe { images.image_symbol::<unsafe extern "C" fn() -> i32>(index, c"attachment_fixture_next") }.is_err()
    );
    assert!(unsafe { images.image_symbol::<unsafe extern "C" fn() -> i32>(2, c"attachment_fixture_second") }.is_err());
    use std::io::Write;
    std::fs::OpenOptions::new().append(true).open(&second).unwrap().write_all(b"changed").unwrap();
    assert!(images.verify_integrity().is_err());
}
