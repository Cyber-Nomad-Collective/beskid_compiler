//! Regression: a Linux x86_64 program that links the Corelib AES and ChaCha20 kernels links and runs.
//!
//! Before the x86-64-v2 target baseline (OpenSpec `compiler--build-pipeline--backends-jit-aot`, "Target CPU
//! baseline"), the AES `swizzle` and ChaCha20 `shuffle` kernels lowered to `__cranelift_x86_pshufb`, which no
//! runtime kit defines, and the link failed with E4021.

use std::process::Command;
use std::time::Duration;

use crate::harness::assertions::{assert_exit_code, assert_file_exists, assert_success};
use crate::harness::cli::BeskidCliInvoker;
use crate::harness::process::run_binary;

const MAIN: &str = r#"use Core.Bytes.Slice;
use Core.Output;
use Core.Results;
use Core.String;
use Crypto.Aes;
use Crypto.ChaCha20;
use Crypto.Errors;

u8 Nibble(u8 c) {
    if c >= 48_u8 && c <= 57_u8 { return c - 48_u8; }
    return c - 87_u8;
}

u8[] Hex(string text) {
    i64 n = String.Len(text) / 2_i64;
    u8[] out = Slice.New(n);
    mut i64 i = 0_i64;
    while i < n {
        out[i] = Nibble(String.ByteAt(text, 2_i64 * i)) * 16_u8 + Nibble(String.ByteAt(text, 2_i64 * i + 1_i64));
        i = i + 1_i64;
    }
    return out;
}

u8[] Range(i64 n) {
    u8[] out = Slice.New(n);
    mut i64 i = 0_i64;
    while i < n { out[i] = u8(i); i = i + 1_i64; }
    return out;
}

bool Matches(Result<u8[], CryptoError> result, string expected) {
    return match result {
        Result::Ok(bytes) => Slice.Compare(bytes, Hex(expected)) == 0_i64,
        Result::Error(_) => false,
    };
}

i32 Main()
{
    // FIPS 197 appendix C.1 (AES-128).
    if !Matches(Aes.Encrypt(Range(16_i64), Hex("00112233445566778899aabbccddeeff")), "69c4e0d86a7b0430d8cdb78070b4c55a") {
        return 11;
    }
    // FIPS 197 appendix C.3 (AES-256).
    if !Matches(Aes.Encrypt(Range(32_i64), Hex("00112233445566778899aabbccddeeff")), "8ea2b7ca516745bfeafc49904b496089") {
        return 12;
    }
    // RFC 8439 section 2.3.2 (ChaCha20 block function).
    if !Matches(
        ChaCha20.Block(Range(32_i64), 1_u32, Hex("000000090000004a00000000")),
        "10f1e7e4d13b5915500fdd1fa32071c4c7d1f4c733c068030422aa9ac3d46c4ed2826446079faa0914c2d705d98b02a2b5129cd1de164eb9cbd083e8a2503c4e"
    ) {
        return 13;
    }
    Output.WriteLine("crypto ok");
    return 0;
}
"#;

#[test]
fn aes_and_chacha20_program_links_and_runs_on_x86_64_linux() {
    let cli = BeskidCliInvoker::new();
    let corelib = cli.corelib_root().join("beskid_corelib");
    let dir = tempfile::Builder::new().prefix("beskid_e2e_crypto_baseline_").tempdir().expect("temp dir");
    std::fs::create_dir_all(dir.path().join("Src")).expect("create Src");
    std::fs::write(
        dir.path().join("CryptoBaseline.bproj"),
        format!(
            "CryptoBaseline {{\n  name = \"CryptoBaseline\"\n  version = \"0.1.0\"\n}}\n\n\
             dependency \"corelib\" {{ source = path path = \"{}\" }}\n\n\
             target \"App\" {{\n  kind = App\n  entry = \"Main.bd\"\n}}\n",
            corelib.display()
        ),
    )
    .expect("write manifest");
    std::fs::write(dir.path().join("Src/Main.bd"), MAIN).expect("write Main.bd");

    let manifest = dir.path().join("CryptoBaseline.bproj");
    let binary = dir.path().join("out/crypto_baseline");
    let object = dir.path().join("out/crypto_baseline.o");
    let build = cli.run_in(
        dir.path(),
        [
            "build",
            "--project",
            manifest.to_str().expect("manifest path"),
            "--target",
            "App",
            "--target-triple",
            "x86_64-unknown-linux-gnu",
            "--output",
            binary.to_str().expect("binary path"),
            "--object-output",
            object.to_str().expect("object path"),
        ],
    );
    assert_success(&build, "build the AES and ChaCha20 program");
    assert_file_exists(&binary, "AES and ChaCha20 binary");

    let undefined = Command::new("nm").arg("--undefined-only").arg(&object).output().expect("run nm");
    assert!(undefined.status.success(), "nm failed for {}", object.display());
    let helpers = String::from_utf8_lossy(&undefined.stdout)
        .lines()
        .filter(|line| line.contains("__cranelift_"))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert!(helpers.is_empty(), "the object needs Cranelift runtime helpers: {helpers:?}");

    let run = run_binary(&binary, Duration::from_secs(60));
    assert_success(&run, "run the AES and ChaCha20 program");
    assert_exit_code(&run, 0, "run the AES and ChaCha20 program");
}
