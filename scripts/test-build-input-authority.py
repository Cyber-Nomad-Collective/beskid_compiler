#!/usr/bin/env python3
"""Exercise actual build-script watch directives with standalone rustc, without Cargo.

Dependency stubs only bypass artifact generation/hashing; watch/copy/resolution code is real.
"""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
MANIFEST_STUB = r'''
pub fn default_manifest_path(root: &std::path::Path) -> std::path::PathBuf { root.join("runtime.bsol") }
pub fn load_v5_manifest_source(_: &str) -> Result<(), String> { Ok(()) }
pub struct Artifacts { pub rust:String, pub c_header:String, pub abi_json:String, pub audit_json:String,
 pub gnu_asm:Vec<(String,String)>, pub masm:Vec<(String,String)> }
pub fn generate_v5_artifacts(_: &()) -> Result<Artifacts,String> { Ok(Artifacts {
 rust:String::new(), c_header:String::new(), abi_json:String::new(), audit_json:String::new(),
 gnu_asm:Vec::new(), masm:Vec::new() }) }
'''
SHA_STUB = r'''
pub trait Digest: Sized { fn new() -> Self; fn update(&mut self, bytes: impl AsRef<[u8]>); fn finalize(self) -> [u8;32]; }
pub struct Sha256;
impl Digest for Sha256 { fn new()->Self { Self } fn update(&mut self,_:impl AsRef<[u8]>) {} fn finalize(self)->[u8;32] { [0;32] } }
'''


class BuildInputAuthority(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="beskid-build-input-test-")
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name)
        self.workspace = self.root / "source"
        for relative in ["beskid_corelib/src", "packages/Numbers/src", "packages/Numbers/docs", "packages/Numbers/obj", "beskid_corelib/obj"]:
            (self.workspace / relative).mkdir(parents=True, exist_ok=True)
        for relative, text in {
            "CoreLib.bws": "workspace {}", "LICENSE": "license", "README.md": "not embedded",
            "beskid_corelib/corelib.bproj": "project {}", "beskid_corelib/src/Main.bd": "source",
            "packages/Numbers/Numbers.bproj": "project {}", "packages/Numbers/src/Value.bd": "source",
            "packages/Numbers/docs/index.md": "docs", "packages/Numbers/Project.lock": "generated",
            "packages/Numbers/obj/diagnostics.json": "generated", "beskid_corelib/obj/generated.bd": "generated",
        }.items():
            (self.workspace / relative).write_text(text)

    def run_script(self, crate):
        manifest = self.root / "checkout" / "crates" / crate
        manifest.mkdir(parents=True, exist_ok=True)
        (manifest / "runtime.bsol").write_text("manifest")
        out = self.root / "out"
        out.mkdir(exist_ok=True)
        env = dict(os.environ, CARGO_MANIFEST_DIR=str(manifest), OUT_DIR=str(out), BESKID_CORELIB_SOURCE=str(self.workspace))
        externs = []
        for name, text in [("beskid_manifest", MANIFEST_STUB)] if crate == "beskid_abi" else [("sha2", SHA_STUB)]:
            source = self.root / f"{name}.rs"
            source.write_text(text)
            library = self.root / f"lib{name}.rlib"
            subprocess.run(["rustc", "--edition=2024", "--crate-name", name, "--crate-type", "rlib", str(source), "-o", str(library)], check=True, capture_output=True, env=env)
            externs += ["--extern", f"{name}={library}"]
        binary = self.root / f"{crate}-build"
        subprocess.run(["rustc", "--edition=2024", str(ROOT / "crates" / crate / "build.rs"), *externs, "-o", str(binary)], check=True, capture_output=True, env=env)
        result = subprocess.run([str(binary)], check=True, capture_output=True, text=True, env=env)
        return {Path(line.split("=", 1)[1]) for line in result.stdout.splitlines() if line.startswith("cargo:rerun-if-changed=")}

    def test_abi_watches_validation_files_not_entire_corelib_trees(self):
        watched = self.run_script("beskid_abi")
        self.assertIn(self.workspace / "CoreLib.bws", watched)
        self.assertFalse(any(path.is_dir() for path in watched), f"ABI generated artifacts do not depend on Corelib source trees: {watched}")
        self.assertFalse(any("obj" in path.parts or path.name == "Project.lock" for path in watched))

    def test_tools_watches_embedded_package_descendants_and_filters_generated_files(self):
        watched = self.run_script("beskid_tools")
        for relative in ["CoreLib.bws", "LICENSE", "beskid_corelib/corelib.bproj", "beskid_corelib/src/Main.bd", "packages/Numbers/Numbers.bproj", "packages/Numbers/src/Value.bd", "packages/Numbers/docs/index.md"]:
            self.assertIn(self.workspace / relative, watched, relative)
        self.assertNotIn(self.workspace / "README.md", watched)
        self.assertFalse(any("obj" in path.parts or path.name == "Project.lock" for path in watched))
        # Cargo directory watches are conservative but required for new/deleted unlisted assets.
        self.assertIn(self.workspace / "packages/Numbers/docs", watched)
        self.assertIn(self.workspace, watched, "new root workspace/legal files must invalidate embedding")
        asset = self.workspace / "packages/Numbers/docs/new-example.txt"
        asset.write_text("new embedded asset")
        self.assertIn(asset, self.run_script("beskid_tools"))
        asset.unlink()
        self.assertNotIn(asset, self.run_script("beskid_tools"))


if __name__ == "__main__":
    unittest.main()
