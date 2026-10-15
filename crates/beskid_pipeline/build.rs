//! Derives `BESKID_GRAMMAR_DIGEST`: a SHA-256 content identity of every source that decides how
//! Beskid text becomes a syntax tree (grammar, Pest drivers, AST construction, parse recovery, macro expansion).
//!
//! `GRAMMAR_REVISION` keys every persisted syntax/AST artifact and Salsa snapshot. A crate version
//! alone does not change when the grammar or tree construction changes, so a newer compiler could
//! adopt trees written by an older one. Hashing the sources makes any such change a new revision.
//!
//! The build script lives here and reads `beskid_analysis` sources by path because
//! `beskid_analysis` depends on this crate; the reverse dependency would form a cycle. The sources
//! are required: a missing input fails the build instead of producing a weaker identity.

use std::fs;
use std::path::{Path, PathBuf};

/// Inputs relative to `crates/beskid_analysis`. Directories contribute every file beneath them.
const TREE_INPUTS: &[&str] = &[
    "src/beskid.pest",
    "src/beskid_doc.pest",
    "src/parser.rs",
    "src/doc_comment_parser.rs",
    "src/parsing",
    "src/syntax",
    "src/macros",
    "src/services/parse.rs",
    "src/services/parse_recovery",
];

/// Hex characters of the digest kept in the revision string (64 bits).
const DIGEST_PREFIX_LEN: usize = 16;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let analysis_root = manifest_dir.join("../beskid_analysis");

    let mut files = Vec::new();
    for input in TREE_INPUTS {
        let path = analysis_root.join(input);
        println!("cargo:rerun-if-changed={}", path.display());
        collect_files(&path, &mut files);
    }
    let mut relative: Vec<(String, PathBuf)> = files
        .into_iter()
        .map(|path| {
            let name = path
                .strip_prefix(&analysis_root)
                .expect("grammar input under beskid_analysis")
                .components()
                .map(|component| component.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            (name, path)
        })
        .collect();
    relative.sort();

    let mut hasher = Sha256::new();
    hasher.update(b"beskid-grammar-revision:1\0");
    for (name, path) in &relative {
        println!("cargo:rerun-if-changed={}", path.display());
        let bytes = fs::read(path).unwrap_or_else(|err| panic!("beskid_pipeline build: read {}: {err}", path.display()));
        // Line endings depend on the checkout, not on the grammar; drop CR so every platform
        // derives the same revision from the same sources.
        let normalized: Vec<u8> = bytes.into_iter().filter(|byte| *byte != b'\r').collect();
        hasher.update(name.as_bytes());
        hasher.update(&[0]);
        hasher.update(&(normalized.len() as u64).to_le_bytes());
        hasher.update(&normalized);
    }
    let digest = hex(&hasher.finish());
    println!("cargo:rustc-env=BESKID_GRAMMAR_DIGEST={}", &digest[..DIGEST_PREFIX_LEN]);
}

fn collect_files(path: &Path, files: &mut Vec<PathBuf>) {
    let metadata = fs::metadata(path)
        .unwrap_or_else(|err| panic!("beskid_pipeline build: grammar input {} is missing: {err}", path.display()));
    if metadata.is_file() {
        files.push(path.to_path_buf());
        return;
    }
    let entries = fs::read_dir(path).unwrap_or_else(|err| panic!("beskid_pipeline build: list {}: {err}", path.display()));
    for entry in entries {
        let entry = entry.unwrap_or_else(|err| panic!("beskid_pipeline build: list {}: {err}", path.display()));
        collect_files(&entry.path(), files);
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// FIPS 180-4 SHA-256. Kept local so the build script adds no dependency (and no lockfile edge);
/// `main` checks it against the standard test vectors before trusting it.
struct Sha256 {
    state: [u32; 8],
    buffer: Vec<u8>,
    length: u64,
}

const ROUND_CONSTANTS: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98,
    0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786,
    0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8,
    0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
    0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819,
    0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a,
    0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
    0xc67178f2,
];

impl Sha256 {
    fn new() -> Self {
        Self::self_test();
        Self::fresh()
    }

    fn fresh() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
            ],
            buffer: Vec::with_capacity(64),
            length: 0,
        }
    }

    fn self_test() {
        let vectors: [(&[u8], &str); 3] = [
            (b"", "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"),
            (b"abc", "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"),
            (
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
                "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
            ),
        ];
        for (input, expected) in vectors {
            let mut hasher = Self::fresh();
            hasher.update(input);
            assert_eq!(hex(&hasher.finish()), expected, "beskid_pipeline build: SHA-256 self-test failed");
        }
    }

    fn update(&mut self, mut bytes: &[u8]) {
        self.length = self.length.wrapping_add(bytes.len() as u64);
        while !bytes.is_empty() {
            let take = (64 - self.buffer.len()).min(bytes.len());
            self.buffer.extend_from_slice(&bytes[..take]);
            bytes = &bytes[take..];
            if self.buffer.len() == 64 {
                let block: [u8; 64] = self.buffer[..].try_into().expect("64-byte block");
                self.compress(&block);
                self.buffer.clear();
            }
        }
    }

    fn finish(mut self) -> [u8; 32] {
        let bit_length = self.length.wrapping_mul(8);
        let mut padding = vec![0x80u8];
        let used = (self.buffer.len() + 1) % 64;
        let zeros = if used <= 56 { 56 - used } else { 120 - used };
        padding.extend(std::iter::repeat_n(0u8, zeros));
        padding.extend_from_slice(&bit_length.to_be_bytes());
        let length = self.length;
        self.update(&padding);
        self.length = length;
        debug_assert!(self.buffer.is_empty());
        let mut out = [0u8; 32];
        for (chunk, word) in out.chunks_exact_mut(4).zip(self.state) {
            chunk.copy_from_slice(&word.to_be_bytes());
        }
        out
    }

    fn compress(&mut self, block: &[u8; 64]) {
        let mut schedule = [0u32; 64];
        for (index, word) in block.chunks_exact(4).enumerate() {
            schedule[index] = u32::from_be_bytes(word.try_into().expect("4-byte word"));
        }
        for index in 16..64 {
            let s0 = schedule[index - 15].rotate_right(7)
                ^ schedule[index - 15].rotate_right(18)
                ^ (schedule[index - 15] >> 3);
            let s1 = schedule[index - 2].rotate_right(17)
                ^ schedule[index - 2].rotate_right(19)
                ^ (schedule[index - 2] >> 10);
            schedule[index] =
                schedule[index - 16].wrapping_add(s0).wrapping_add(schedule[index - 7]).wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choose = (e & f) ^ (!e & g);
            let t1 = h.wrapping_add(s1).wrapping_add(choose).wrapping_add(ROUND_CONSTANTS[index]).wrapping_add(schedule[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (slot, value) in self.state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *slot = slot.wrapping_add(value);
        }
    }
}
