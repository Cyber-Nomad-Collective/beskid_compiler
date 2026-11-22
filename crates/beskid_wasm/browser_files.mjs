const fileOperations = ["read_text", "write_text", "exists", "mkdir", "delete"];
const filePrefix = "beskid_rt_v5_fs_";
const fileStatus = Object.freeze({ ok: 0, missing: 1, denied: 2, io: 3, invalid: 4, exists: 5 });
const originFileRoot = "opfs:/";

function fileParts(root, path) {
  if (typeof path !== "string" || !path.startsWith(root) || /[\\\u0000]/u.test(path)) return null;
  const suffix = path.slice(root.length).replace(/^\/+/, "");
  if (suffix === "") return [];
  const parts = suffix.split("/");
  return parts.some(part => !part || part === "." || part === "..") ? null : parts;
}

async function hasFileEntry(directory, name) {
  try {
    await directory.getFileHandle(name);
    return true;
  } catch (error) {
    if (error.name === "NotFoundError") return false;
    if (error.name !== "TypeMismatchError") throw error;
  }
  try {
    await directory.getDirectoryHandle(name);
    return true;
  } catch (error) {
    if (error.name === "NotFoundError") return false;
    throw error;
  }
}

async function runFileOperation(op, root, path, text, signal, getRoot) {
  const respond = (status, text) => op === "read_text" ? { status, text } : status;
  const parts = fileParts(root, path);
  if (!parts || (!parts.length && op !== "exists")) return respond(fileStatus.invalid);
  const mutates = op !== "read_text" && op !== "exists";
  try {
    signal.throwIfAborted();
    let directory = await getRoot(mutates);
    signal.throwIfAborted();
    if (!directory) return respond(fileStatus.denied);
    const leaf = parts.pop();
    if (leaf === undefined) return respond(fileStatus.ok);
    for (const segment of parts) {
      directory = await directory.getDirectoryHandle(segment);
      signal.throwIfAborted();
    }
    switch (op) {
      case "mkdir":
        if (await hasFileEntry(directory, leaf)) return respond(fileStatus.exists);
        signal.throwIfAborted();
        await directory.getDirectoryHandle(leaf, { create: true });
        break;
      case "delete":
        await directory.removeEntry(leaf);
        break;
      case "exists":
        return respond(await hasFileEntry(directory, leaf) ? fileStatus.ok : fileStatus.missing);
      case "write_text": {
        const handle = await directory.getFileHandle(leaf, { create: true });
        signal.throwIfAborted();
        const writer = await handle.createWritable();
        try {
          signal.throwIfAborted();
          await writer.write(text);
          signal.throwIfAborted();
          await writer.close();
        } catch (error) {
          await writer.abort().catch(() => {});
          throw error;
        }
        break;
      }
      case "read_text": {
        const handle = await directory.getFileHandle(leaf);
        signal.throwIfAborted();
        const file = await handle.getFile();
        const bytes = await file.arrayBuffer();
        signal.throwIfAborted();
        const decoded = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(bytes);
        return respond(fileStatus.ok, decoded);
      }
    }
    return respond(fileStatus.ok);
  } catch (error) {
    signal.throwIfAborted();
    const statuses = { NotFoundError: fileStatus.missing, NotAllowedError: fileStatus.denied,
      SecurityError: fileStatus.denied, TypeMismatchError: fileStatus.invalid };
    return respond(statuses[error?.name] ?? fileStatus.io);
  }
}

function fileMethod(op, root, getRoot) {
  return (path, ...args) => {
    const signal = args.pop();
    return runFileOperation(op, root, path, args[0], signal, getRoot);
  };
}

function originFile(op, path, ...args) {
  return fileMethod(op, originFileRoot, () => navigator.storage.getDirectory())(path, ...args);
}

/** Turn a directory handle explicitly granted by the application into a file adapter. */
export function createFileDirectoryAdapter(handle, { root = "mount:/", readOnly = false } = {}) {
  check(typeof FileSystemDirectoryHandle === "function" && handle instanceof FileSystemDirectoryHandle &&
    typeof handle.queryPermission === "function", "Expected a browser FileSystemDirectoryHandle with permission queries");
  check(typeof root === "string" && /^[A-Za-z][A-Za-z0-9+.-]*:\/$/u.test(root), "Expected an explicit virtual file root");
  check(typeof readOnly === "boolean", "Expected a boolean readOnly file adapter option");
  const getRoot = async mutates => {
    if (mutates && readOnly) return null;
    const mode = mutates ? "readwrite" : "read";
    return await handle.queryPermission({ mode }) === "granted" ? handle : null;
  };
  const methods = Object.freeze(Object.fromEntries(
    fileOperations.map(op => [filePrefix + op, fileMethod(op, root, getRoot)])),
  );
  return Object.freeze({ service: "files", version: 1, methods });
}
