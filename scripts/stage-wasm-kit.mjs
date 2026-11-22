// Stage verified Binaryen release files during toolchain packaging. A local archive permits offline staging.
import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import { copyFile, mkdir, mkdtemp, open, readFile, rename, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { execFileSync } from "node:child_process";

const [target, prefix, archivePath] = process.argv.slice(2);
if (!target || !prefix) throw new Error("usage: stage-wasm-kit.mjs <build-host-triple> <prefix> [Binaryen archive]");
const source = new URL("../crates/beskid_wasm/", import.meta.url);
const manifest = JSON.parse(await readFile(new URL("binaryen-kit.json", source)));
const platform = manifest.platforms[target];
if (!platform) throw new Error("unsupported Wasm compiler host " + target);

async function check(file, expected) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(file)) hash.update(chunk);
  if (hash.digest("hex") !== expected) throw new Error("Binaryen checksum mismatch: " + file);
}

const cache = await mkdtemp(join(tmpdir(), "beskid-binaryen-"));
const parent = resolve(prefix, "lib/beskid-wasm", String(manifest.version));
await mkdir(parent, { recursive: true });
const staging = await mkdtemp(join(parent, ".incoming-"));
try {
  const archive = archivePath ? resolve(archivePath) : join(cache, platform.archive);
  if (!archivePath) {
    const url = "https://github.com/WebAssembly/binaryen/releases/download/version_" + manifest.version + "/" + platform.archive;
    execFileSync("curl", ["--fail", "--location", "--silent", "--show-error", "--retry", "2", "--output", archive, url]);
  }
  await check(archive, platform.sha256);
  const files = Object.entries(platform.files);
  const archiveFile = await open(archive, "r");
  try {
    // GNU tar can interpret a Windows drive prefix as a remote archive name.
    execFileSync("tar", ["-xzf", "-", "--strip-components=1", ...files.map(([name]) => "binaryen-version_" + manifest.version + "/" + name)], {
      cwd: staging,
      stdio: [archiveFile.fd, "pipe", "pipe"],
    });
  } finally {
    await archiveFile.close();
  }
  for (const [name, sha] of files) await check(join(staging, name), sha);
  await copyFile(new URL("LICENSE.binaryen", source), join(staging, "LICENSE.binaryen"));
  const destination = join(parent, target);
  try {
    await rename(staging, destination);
  } catch (error) {
    if (!["ENOTEMPTY", "EEXIST", "EPERM"].includes(error.code)) throw error;
    for (const [name, sha] of files) await check(join(destination, name), sha);
    if (!(await readFile(join(destination, "LICENSE.binaryen"))).equals(await readFile(join(staging, "LICENSE.binaryen")))) throw error;
  }
  console.log("Verified browser compiler tools at " + destination);
} finally {
  await rm(staging, { recursive: true, force: true });
  await rm(cache, { recursive: true, force: true });
}
