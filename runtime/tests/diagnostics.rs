//! Compile real downstream callers and assert the reason for rejection.
//! No snapshot library or network access is needed. rustc writes only to target/.
use std::{fs, path::PathBuf, process::Command};

fn find_library(root: &std::path::Path) -> Option<PathBuf> {
    let mut pending = vec![root.to_owned()];
    while let Some(directory) = pending.pop() {
        let entries = fs::read_dir(directory).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let name = path.file_name()?.to_string_lossy();
            if (name.starts_with("libprobe_ui-") || name.starts_with("probe_ui-"))
                && (name.ends_with(".rlib") || name.ends_with(".rmeta"))
            {
                return Some(path);
            }
        }
    }
    None
}

#[test]
fn invalid_programs_have_the_expected_diagnostic() {
    let deps = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned();
    let target = std::env::current_exe()
        .unwrap()
        .ancestors()
        .find(|path| path.file_name().is_some_and(|name| name == "target"))
        .map(PathBuf::from)
        .unwrap_or_else(|| deps.clone());
    let library =
        find_library(&target).expect("Cargo must build probe-ui before its integration tests");
    let dependency_dir = library.parent().expect("probe-ui library directory");
    let output = dependency_dir.parent().unwrap().join("pattern-diagnostics");
    fs::create_dir_all(&output).unwrap();
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/ui");
    let mut cases: Vec<_> = fs::read_dir(fixtures)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "rs"))
        .collect();
    cases.sort();
    assert!(
        cases.len() >= 12,
        "diagnostic fixtures must not silently disappear"
    );
    for case in &cases {
        let text = fs::read_to_string(case).unwrap();
        let expected: Vec<_> = text
            .lines()
            .filter_map(|line| line.strip_prefix("// error: "))
            .collect();
        assert!(
            !expected.is_empty(),
            "{} lacks error markers",
            case.display()
        );
        let result = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
            .arg(case)
            .args(["--edition=2024", "--crate-type=lib", "--emit=metadata"])
            .arg("--extern")
            .arg(format!("probe_ui={}", library.display()))
            .arg("-L")
            .arg(format!("dependency={}", dependency_dir.display()))
            .arg("--out-dir")
            .arg(&output)
            .env("CARGO_MANIFEST_DIR", env!("CARGO_MANIFEST_DIR"))
            .output()
            .unwrap();
        let diagnostic = String::from_utf8_lossy(&result.stderr);
        assert!(
            !result.status.success(),
            "{} unexpectedly compiled",
            case.display()
        );
        for marker in expected {
            assert!(
                diagnostic.contains(marker),
                "{} must report {marker:?}, got:\n{diagnostic}",
                case.display(),
            );
        }
        println!(
            "PASS diagnostic: {}",
            case.file_name().unwrap().to_string_lossy()
        );
    }
    println!("PASS: {} diagnostic fixtures", cases.len());
}
