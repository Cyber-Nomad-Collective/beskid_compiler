const yieldImport = "beskid_rt_v5_fiber_yield";
const sleepImport = "beskid_rt_v5_external_sleep_until";
const monotonicClock = "clock_monotonic_nanos";
const writeImport = "syscall_write";
const trapImport = "beskid_trap_message";
const formatFloatImport = "str_from_f64_bits";
const scopeGet = "__beskid_scope_get";
const scopeSet = "__beskid_scope_set";
const scopeNew = "__beskid_scope_new";
const fiberGet = "__beskid_fiber_current_get";
const fiberSet = "__beskid_fiber_current_set";
const referenceHelpers = ["__beskid_reference_get", "__beskid_reference_intern"];
const byteHelpers = ["array_i8_alloc", "array_i8_len", "array_i8_get", "array_i8_set"];
const wordHelpers = ["array_i64_len", "array_i64_set"];
const scopeHelpers = [scopeGet, scopeSet, scopeNew];
const stringHelpers = [
  "browser_string_new_len", "browser_string_len", "browser_string_byte_get", "browser_string_byte_set", "browser_string_assign",
];
const version = 2;
const signedI64Min = -(1n << 63n);
const signedI64Max = (1n << 63n) - 1n;
const wordMax = (1n << 64n) - 1n;
const wasmTypes = new Map([
  ["bool", "i32"], ["char", "i32"], ["u8", "i32"], ["i32", "i32"],
  ["i8", "i32"], ["i16", "i32"], ["u16", "i32"], ["u64", "i64"], ["f32", "f32"], ["u32", "i32"], ["i64", "i64"], ["word", "i64"], ["f64", "f64"], ["string", "i64"], ["bytes", "i64"], ["managed", "i64"],
]);

const unaryMath = ["sin", "cos", "tan", "log", "log2", "log10"];
const methods = new Map([
  ...browserProtocolMethods,
  ...browserNetworkMethods,
  ...browserSynchronizationMethods,
  ...browserCompositionMethods,
  ["__beskid_dynamic_token_issue", { id: "dynamic", parameters: ["i64"], results: ["i64"], overridable: false }],
  ["__beskid_dynamic_token_resolve", { id: "dynamic", parameters: ["i64"], results: ["i64"], overridable: false }],
  ["gc_collect", { id: "testCollector", parameters: [], results: ["i64"], requiresAdapter: true }],
  ["fiber_spawn", { id: "scheduler", parameters: ["i64", "i64"], results: ["i64"], managedTransfer: true, overridable: false }],
  ["__beskid_fiber_stack_enter", { id: "scheduler", parameters: ["i64"], results: [], overridable: false }],
  ["__beskid_fiber_stack_leave", { id: "scheduler", parameters: ["i64"], results: [], overridable: false }],
  ["fiber_cancel", { id: "scheduler", parameters: ["i64", "i64"], results: ["i32"], overridable: false }],
  ["fiber_detach", { id: "scheduler", parameters: ["i64"], results: [], overridable: false }],
  ["fiber_join_status", { id: "scheduler", parameters: ["i64"], results: ["i32"], maySuspend: true, overridable: false }],
  ["fiber_join_value", { id: "scheduler", parameters: ["i64"], results: ["i64"], overridable: false }],
  ["fiber_join_detail", { id: "scheduler", parameters: ["i64", "i64"], results: ["i64"], overridable: false }],
  ["fiber_join_message", { id: "scheduler", parameters: ["i64"], results: ["i64"], stringResult: true, overridable: false }],
  ["fiber_join_error_finish", { id: "scheduler", parameters: ["i64"], results: ["i32"], overridable: false }],
  ["fiber_processor_count", { id: "runtime", parameters: [], results: ["i64"], overridable: false, invoke: () => 1n }],
  ["tty_winsize", { id: "terminal", parameters: ["i64"], results: ["i64"], invoke: () => 0n }],
  ...["syscall_read", "syscall_read_bytes", "syscall_write_bytes"].map(name => {
    const read = name !== "syscall_write_bytes";
    return [name, { id: "streams", parameters: ["i32", "i64", "i64"], results: ["i64"],
      maySuspend: read, readBytes: read, bytes: read ? [] : [1], invoke: () => -1n }];
  }),
  ["beskid_rt_browser_entropy_fill", { id: "entropy", parameters: ["i64"], results: ["i64"], bytes: [0],
    writeBytes: 0, invoke: buffer => {
      try {
        for (let offset = 0; offset < buffer.length; offset += 65536) {
          globalThis.crypto.getRandomValues(buffer.subarray(offset, offset + 65536));
        }
        return 0n;
      } catch {
        return 1n;
      }
    } }],
  ["beskid_rt_v5_args_count", { id: "arguments", parameters: [], results: ["i64"], invoke: () => 0n }],
  ["beskid_rt_v5_args_get", { id: "arguments", parameters: ["i64"], results: ["i64"], stringResult: true,
    invoke: () => { throw new RangeError("Argument index is outside the virtual argument list"); } }],
  ["env_get", { id: "environment", parameters: ["i64"], results: ["i64"], strings: [0], stringResult: true }],
  ["env_set", { id: "environment", parameters: ["i64", "i64"], results: ["i32"], strings: [0, 1] }],
  ["env_getcwd", { id: "environment", parameters: [], results: ["i64"], stringResult: true, invoke: () => "opfs:/" }],
  ...fileOperations.map(op => {
    const write = op === "write_text", read = op === "read_text";
    return [filePrefix + op, { id: "files", parameters: write || read ? ["i64", "i64"] : ["i64"],
      results: ["i32"], strings: write ? [0, 1] : [0], maySuspend: true, readText: read,
      invoke: (...args) => originFile(op, ...args) }];
  }),
  ...unaryMath.map(name => [name, { id: "math", parameters: ["f64"], results: ["f64"], invoke: Math[name] }]),
  ["atan2", { id: "math", parameters: ["f64", "f64"], results: ["f64"], invoke: Math.atan2 }],
  [monotonicClock, { id: "clock", parameters: [], results: ["i64"],
    invoke: () => BigInt(Math.floor(performance.now() * 1e6)) }],
  ["clock_realtime_nanos", { id: "clock", parameters: [], results: ["i64"],
    invoke: () => BigInt(Date.now()) * 1_000_000n }],
  ["fiber_now_millis", { id: "clock", parameters: [], results: ["i64"],
    invoke: () => BigInt(Math.floor(performance.now())) }],
  [yieldImport, { id: "scheduler", parameters: [], results: [], maySuspend: true, invoke: queueTask }],
  [sleepImport, { id: "scheduler", parameters: ["i64"], results: ["i64"], maySuspend: true }],
  [writeImport, { id: "console", parameters: ["i64", "i64"], results: ["i64"], stringArg: 1, invoke: writeConsole }],
  [trapImport, { id: "runtime", parameters: ["i64"], results: [], stringArg: 0, overridable: false,
    invoke: message => { throw new WebAssembly.RuntimeError(message); } }],
  ["beskid_trap_code", { id: "runtime", parameters: ["i64"], results: [], overridable: false,
    invoke: code => { throw new WebAssembly.RuntimeError(`Beskid panic (${code})`); } }],
  [formatFloatImport, { id: "runtime", parameters: ["i64"], results: ["i64"], stringResult: true, overridable: false,
    invoke: formatFloat }],
]);

function check(test, message) {
  if (!test) throw new TypeError(message);
}

function equalArray(actual, expected) {
  return Array.isArray(actual) && actual.length === expected.length &&
    actual.every((value, index) => value === expected[index]);
}

function readProfile(module) {
  const sections = WebAssembly.Module.customSections(module, "beskid.browser");
  check(sections.length === 1, "Expected one beskid.browser profile section");
  let profile;
  try {
    profile = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(sections[0]));
  } catch (cause) {
    throw new TypeError("Invalid beskid.browser profile section", { cause });
  }
  check(profile?.formatVersion === version && profile.hostInterfaceVersion === version,
    `Incompatible Beskid browser profile. Expected format and host interface version ${version}`);
  check(Array.isArray(profile.requiredFeatures) && Array.isArray(profile.services) &&
    Array.isArray(profile.exports) && profile.exports.length > 0 &&
    Array.isArray(profile.internalExports) && typeof profile.managedScopes === "boolean",
  "Beskid browser profile is missing features, services or exports");

  const features = new Set(profile.requiredFeatures);
  check(features.size === profile.requiredFeatures.length, "Duplicate Beskid browser feature");
  for (const feature of features) {
    // The engine already validated the compiled module's Wasm features.
    if (feature === "wasm-gc" || feature === "simd") continue;
    if (feature === "jspi") {
      check(typeof WebAssembly.Suspending === "function" && typeof WebAssembly.promising === "function",
        "Beskid browser module requires WebAssembly JavaScript Promise Integration (JSPI)");
    } else {
      throw new TypeError(`Unsupported Beskid browser feature ${String(feature)}`);
    }
  }

  const moduleImports = WebAssembly.Module.imports(module);
  const imports = new Set(moduleImports.map(({ module, name, kind }) =>
    JSON.stringify([module, name, kind])));
  check(imports.size === moduleImports.length, "Beskid browser module has duplicate imports");
  let copyStrings = false;
  let copyBytes = false;
  let copyWords = false;
  let managed = false;
  for (const service of profile.services) {
    const { id, module, name, version, parameters, results, required, maySuspend } = service ?? {};
    const method = methods.get(name);
    const namespace = profile.managedScopes && method?.maySuspend ? "beskid" : "env";
    check(method && method.id === id && module === namespace && version === 1 && required === true &&
      equalArray(parameters, method.parameters) && equalArray(results, method.results) &&
      maySuspend === Boolean(method.maySuspend), `Unsupported Beskid browser service descriptor ${String(name)}`);
    check(imports.delete(JSON.stringify([module, name, "function"])),
      `Duplicate or absent Beskid browser import ${module}.${name}`);
    check(!maySuspend || features.has("jspi"), `Beskid browser service ${name} requires JSPI`);
    copyStrings ||= Boolean(method.strings) || method.stringArg !== undefined || method.stringResult === true;
    copyBytes ||= Boolean(method.bytes?.length || method.readBytes);
    copyBytes ||= method.networkBytes === true;
    copyWords ||= method.networkWords === true;
    copyStrings ||= method.networkString === true;
    managed ||= method.managedTransfer === true;
  }
  check(imports.size === 0, "Beskid browser module has undeclared imports");

  const functions = new Set(WebAssembly.Module.exports(module).filter(({ kind }) => kind === "function")
    .map(({ name }) => name));
  for (const entry of profile.exports) {
    const { name, parameters, results, maySuspend, awaitChildren = false } = entry ?? {};
    check(functions.delete(name), `Duplicate or absent Beskid browser export ${String(name)}`);
    check(Array.isArray(parameters) && Array.isArray(results) && results.length <= 1 &&
      [...parameters, ...results].every(value =>
        wasmTypes.has(value?.type) && wasmTypes.get(value.type) === value.wasmType &&
        (value.type !== "managed" || (Number.isSafeInteger(value.typeId) && value.typeId >= 0))),
    `Unsupported function signature for Beskid browser export ${name}`);
    check(typeof maySuspend === "boolean" && (!maySuspend || features.has("jspi")) &&
      typeof awaitChildren === "boolean" && (!awaitChildren || (maySuspend && profile.services.some(service => service.name === "fiber_spawn"))),
      `Invalid suspension metadata for Beskid browser export ${name}`);
    copyBytes ||= [...parameters, ...results].some(value => value.type === "bytes");
    managed ||= [...parameters, ...results].some(value => value.type === "managed");
    copyStrings ||= [...parameters, ...results].some(value => value.type === "string");
  }
  check(!(copyStrings || copyBytes || copyWords || managed) || profile.managedScopes, "Beskid browser transfer requires a managed scope");
  const internalExports = profile.managedScopes ? [...scopeHelpers] : [];
  const fiberEntries = profile.internalExports.filter(name => /^__beskid_fiber_entry_[1-9][0-9]*$/.test(name));
  if (fiberEntries.length) {
    check(profile.services.some(service => service.name === "fiber_spawn") && features.has("jspi"), "Fiber entries require a scheduler and JSPI");
    for (let i = 1; i <= fiberEntries.length; i++) check(fiberEntries.includes(`__beskid_fiber_entry_${i}`), "Fiber entries must be contiguous");
    internalExports.push(...fiberEntries, fiberGet, fiberSet);
  }
  const compositionHooks = profile.internalExports.filter(name => /^__beskid_composition_dispose_[1-9][0-9]*_[0-2]$/.test(name));
  if (compositionHooks.length) {
    check(profile.managedScopes && profile.services.some(service => service.name === "composition_exit_register"),
      "Composition hooks require the source-scoped composition runtime");
    internalExports.push(...compositionHooks);
  }
  if (managed || profile.internalExports.includes(referenceHelpers[0])) internalExports.push(...referenceHelpers);
  if (copyBytes || profile.internalExports.includes(byteHelpers[0])) {
    check(profile.managedScopes, "Beskid browser byte helpers require a managed scope");
    internalExports.push(...byteHelpers);
  }
  if (copyWords || profile.internalExports.includes(wordHelpers[0])) {
    check(profile.managedScopes, "Beskid browser word helpers require a managed scope");
    internalExports.push(...wordHelpers);
  }
  if (copyStrings || profile.internalExports.includes(stringHelpers[0])) {
    check(profile.managedScopes, "Beskid browser UTF-8 helpers require a managed scope");
    internalExports.push(...stringHelpers);
  }
  check(equalArray([...profile.internalExports].sort(), internalExports.sort()),
    "Unexpected or missing Beskid browser internal exports");
  for (const name of profile.internalExports) {
    check(functions.delete(name), `Duplicate or absent Beskid browser helper ${name}`);
  }
  check(functions.size === 0, "Beskid browser module has undeclared function exports");
  return profile;
}

function bindAdapters(adapters, services) {
  check(adapters && typeof adapters === "object" && !Array.isArray(adapters),
    "Expected an object of Beskid browser adapters");
  const bindings = new Map();
  const teardown = [];
  for (const [id, adapter] of Object.entries(adapters)) {
    const known = [...methods].filter(([, method]) => method.id === id && method.overridable !== false);
    check(known.length > 0, `Unknown Beskid browser service ${id}`);
    check(adapter?.service === id && adapter.version === 1,
      `Incompatible Beskid browser adapter for ${id}. Expected version 1`);
    check(adapter.methods && typeof adapter.methods === "object" && !Array.isArray(adapter.methods),
      `Beskid browser adapter for ${id} requires a methods object`);
    check(adapter.dispose === undefined || typeof adapter.dispose === "function",
      `Beskid browser adapter for ${id} requires a callable dispose method`);
    for (const [name, invoke] of Object.entries(adapter.methods)) {
      const method = methods.get(name);
      check(method?.id === id && method.overridable !== false && typeof invoke === "function",
        `Invalid method ${name} for Beskid browser adapter ${id}`);
      if (id === "network") bindings.set(name, invoke.bind(adapter.methods));
    }
    for (const service of services.filter(service => service.id === id)) {
      if (id === "network") continue;
      check(Object.hasOwn(adapter.methods, service.name) && typeof adapter.methods[service.name] === "function",
        `Beskid browser adapter for ${id} is missing ${service.name}`);
      bindings.set(service.name, adapter.methods[service.name].bind(adapter.methods));
    }
    if (adapter.dispose) teardown.push(adapter.dispose.bind(adapter));
  }
  return { bindings, teardown };
}

function queueTask(signal) {
  return waitForTask(0, signal);
}

async function sleepUntil(deadline, signal, clock) {
  const maxTimerMillis = 2_147_483_647n;
  const nanosPerMilli = 1_000_000n;
  while (true) {
    signal.throwIfAborted();
    const now = checkedResult(clock(), { name: monotonicClock, results: ["i64"] });
    if (now < 0n) return 0n; // Corelib TimerError::Unavailable.
    if (deadline <= now) return 4n; // Corelib Core.Time.Sleep success.
    const millis = (deadline - now + nanosPerMilli - 1n) / nanosPerMilli;
    await waitForTask(Number(millis > maxTimerMillis ? maxTimerMillis : millis), signal);
  }
}

function waitForTask(millis, signal) {
  signal.throwIfAborted();
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      signal.removeEventListener("abort", onAbort);
      resolve();
    }, millis);
    const onAbort = () => {
      clearTimeout(timer);
      reject(signal.reason);
    };
    signal.addEventListener("abort", onAbort, { once: true });
  if (signal.aborted) onAbort();
  });
}

function checkedResult(value, service) {
  switch (service.results[0]) {
    case "i64":
      check(typeof value === "bigint" && value >= signedI64Min && value <= signedI64Max,
        `Beskid browser adapter method ${service.name} must return a signed i64 BigInt`);
      break;
    case "i32":
      check(Number.isInteger(value) && value >= -2147483648 && value <= 2147483647,
        "Beskid browser adapter must return a signed i32");
      break;
    case "f64":
      check(typeof value === "number", `Beskid browser adapter method ${service.name} must return a number`);
      break;
    default:
      check(value === undefined, `Beskid browser adapter method ${service.name} must return undefined`);
  }
  return value;
}

function inRange(value, min, max) {
  return typeof value === "number" && Number.isInteger(value) && value >= min && value <= max;
}

function valueForWasm(type, value, name, strings) {
  let valid;
  switch (type) {
    case "i64":
      valid = typeof value === "bigint" && value >= signedI64Min && value <= signedI64Max;
      break;
    case "u64":
    case "word":
      valid = typeof value === "bigint" && value >= 0n && value <= wordMax;
      break;
    case "f32":
    case "f64":
      valid = typeof value === "number";
      break;
    case "i32":
      valid = inRange(value, -0x80000000, 0x7fffffff);
      break;
    case "u32":
      valid = inRange(value, 0, 0xffffffff);
      break;
    case "i8":
      valid = inRange(value, -128, 127);
      break;
    case "i16":
      valid = inRange(value, -32768, 32767);
      break;
    case "u16":
      valid = inRange(value, 0, 65535);
      break;
    case "u8":
      valid = inRange(value, 0, 0xff);
      break;
    case "char":
      valid = inRange(value, 0, 0x10ffff) && !inRange(value, 0xd800, 0xdfff);
      break;
    case "bool":
      valid = typeof value === "boolean";
      break;
    case "string":
      valid = typeof value === "string" && value.isWellFormed();
      break;
  }
  check(valid, `Beskid browser export ${name} expects a ${type} argument`);
  if (type === "string") return strings.copyIn(value);
  return type === "bool" ? Number(value) : value;
}

function valueFromWasm(type, value, strings) {
  switch (type) {
    case "bool": return Boolean(value);
    case "i8": return (value << 24) >> 24;
    case "i16": return (value << 16) >> 16;
    case "u16": return value & 65535;
    case "u8": return value & 0xff;
    case "u32": return value >>> 0;
    case "u64":
    case "word": return BigInt.asUintN(64, value);
    case "string": return strings.copyOut(value);
    default: return value;
  }
}

function stringTransfer(wasm) {
  const encode = new TextEncoder();
  const decode = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true });
  return {
    copyIn(text) {
      const bytes = encode.encode(text);
      const handle = wasm.browser_string_new_len(bytes.length);
      for (let index = 0; index < bytes.length; index++) {
        wasm.browser_string_byte_set(handle, index, bytes[index]);
      }
      return BigInt(handle >>> 0);
    },
    copyOut(handle) {
      const index = Number(handle);
      const bytes = new Uint8Array(wasm.browser_string_len(index) >>> 0);
      for (let offset = 0; offset < bytes.length; offset++) {
        bytes[offset] = wasm.browser_string_byte_get(index, offset);
      }
      return decode.decode(bytes);
    },
  };
}

function formatFloat(bits) {
  const view = new DataView(new ArrayBuffer(8));
  view.setBigInt64(0, bits);
  // The native formatter uses the ECMAScript decimal layout for IEEE-754 values.
  return String(view.getFloat64(0));
}

function writeConsole(descriptor, text) {
  if (descriptor === 1n) console.log(text);
  else if (descriptor === 2n) console.error(text);
  else return -1n;
  return BigInt(new TextEncoder().encode(text).length);
}

function withAbort(invoke, args, pending, fiberSignal) {
  const controller = new AbortController();
  const { signal } = controller;
  pending.add(controller);
  const onFiberAbort = () => controller.abort(fiberSignal.reason);
  if (fiberSignal?.aborted) onFiberAbort();
  else fiberSignal?.addEventListener("abort", onFiberAbort, { once: true });
  const aborted = Promise.withResolvers();
  const onAbort = () => aborted.reject(signal.reason);
  signal.addEventListener("abort", onAbort, { once: true });
  const result = Promise.resolve().then(() => {
    signal.throwIfAborted();
    return invoke(...args, signal);
  });
  return Promise.race([result, aborted.promise]).finally(() => {
    signal.removeEventListener("abort", onAbort);
    fiberSignal?.removeEventListener("abort", onFiberAbort);
    pending.delete(controller);
  });
}

function managedTransfer(wasm) {
  let objects = new WeakMap();
  let wrappers = new WeakMap();
  function release() {
    const held = objects.get(this);
    if (held) {
      wrappers.delete(held.object);
      objects.delete(this);
    }
  }
  return {
    copyOut(handle, typeId) {
      const object = wasm.__beskid_reference_get(Number(handle));
      check(object !== null, "Beskid returned an empty managed reference");
      let wrapper = wrappers.get(object);
      if (!wrapper) {
        wrapper = Object.freeze({ release });
        objects.set(wrapper, { object, typeId });
        wrappers.set(object, wrapper);
      }
      check(objects.get(wrapper).typeId === typeId, "Unexpected Beskid managed result type");
      return wrapper;
    },
    copyIn(wrapper, typeId) {
      const held = objects.get(wrapper);
      check(held && held.typeId === typeId, "Expected a live managed reference of this type and instance");
      return BigInt(wasm.__beskid_reference_intern(held.object, 0) >>> 0);
    },
    clear() {
      objects = new WeakMap();
      wrappers = new WeakMap();
    },
  };
}

function byteTransfer(wasm) {
  return {
    copyIn(value) {
      check(ArrayBuffer.isView(value) && Object.prototype.toString.call(value) === "[object Uint8Array]",
        "Expected Uint8Array for a Beskid byte buffer");
      const handle = wasm.array_i8_alloc(value.length);
      for (let i = 0; i < value.length; i++) wasm.array_i8_set(handle, i, value[i]);
      return BigInt(handle >>> 0);
    },
    copyOut(value) {
      const handle = Number(value);
      const result = new Uint8Array(wasm.array_i8_len(handle) >>> 0);
      for (let i = 0; i < result.length; i++) result[i] = wasm.array_i8_get(handle, i);
      return result;
    },
  };
}

let dynamicTokenSeed;
let dynamicTokenSequence = 0n;

function nextDynamicToken() {
  dynamicTokenSeed ??= globalThis.crypto.getRandomValues(new BigUint64Array(1))[0];
  let token;
  do {
    check(dynamicTokenSequence <= wordMax, "Beskid Dynamic token space is exhausted");
    token = BigInt.asIntN(64, dynamicTokenSeed + dynamicTokenSequence++);
  } while (token === 0n);
  return token;
}

function createDynamicRegistry() {
  const issued = new Map();
  const resolved = new Map();
  return {
    issue(ordinal) {
      const previous = issued.get(ordinal);
      if (previous !== undefined) return previous;
      const token = nextDynamicToken();
      issued.set(ordinal, token);
      resolved.set(token, ordinal);
      return token;
    },
    resolve: token => resolved.get(token) ?? 0n,
    clear() {
      issued.clear();
      resolved.clear();
    },
  };
}

/** Load a compiled Beskid module with an instance-scoped service environment. */
export function instantiateBrowser(module, { adapters = {} } = {}) {
  const profile = readProfile(module);
  const { bindings, teardown } = bindAdapters(adapters, profile.services);
  const pending = new Set();
  const imports = { env: Object.create(null), beskid: Object.create(null) };
  const capabilities = Object.create(null);
  const environment = new Map();
  const protocol = profile.services.some(service => methods.get(service.name).operation) ? createBrowserIo() : undefined;
  if (protocol) teardown.push(protocol.dispose);
  const network = profile.services.some(service => service.id === "network") ? createBrowserNetwork(bindings) : undefined;
  if (network) teardown.unshift(network.dispose);
  const dynamic = profile.services.some(service => service.id === "dynamic") ? createDynamicRegistry() : undefined;
  if (dynamic) teardown.push(dynamic.clear);
  const defaults = {
    __beskid_dynamic_token_issue: dynamic?.issue,
    __beskid_dynamic_token_resolve: dynamic?.resolve,
    env_get: name => environment.get(name) ?? "",
    env_set: (name, value) => { environment.set(name, value); return 0; },
  };
  let strings;
  let disposed = false;
  let fatalError;
  const requireActive = () => {
    if (fatalError) throw fatalError;
    if (disposed) throw new DOMException("Beskid browser instance is disposed", "InvalidStateError");
  };
  class FiberStackOverflow extends Error {
    constructor(limit, requested) { super("Fiber logical stack budget exceeded"); this.limit = limit; this.requested = requested; }
  }
  const fiberStackLimit = 8n * 1024n * 1024n;
  const references = {
    get: value => wasmExports.__beskid_reference_get(Number(value)),
    intern: (value, hint) => BigInt(wasmExports.__beskid_reference_intern(value, hint) >>> 0),
  };
  const synchronization = createBrowserSynchronization(references, queueTask);
  const composition = createBrowserComposition(
    () => scopes?.get(), references, (entry, arity) => wasmExports[`__beskid_composition_dispose_${entry}_${arity}`],
    profile.services.some(service => service.name === "composition_exit"),
    (scope, reason) => {
      for (const task of fiberTasks.values()) if (task.scope === scope) task.controller.abort(reason);
    },
  );
  const operationSignal = (scope, fiber) => {
    const scopeSignal = composition.signal(scope);
    const fiberSignal = fiberTasks.get(fiber)?.controller.signal;
    return scopeSignal && fiberSignal ? AbortSignal.any([scopeSignal, fiberSignal]) : scopeSignal ?? fiberSignal;
  };
  const fiberTasks = new Map();
  let nextFiber = 1n;
  const fiberTask = id => { const task = fiberTasks.get(id); check(task && !task.consumed, "Invalid or consumed fiber handle"); return task; };
  function ownedByScope(task, scope) {
    if (task.scope !== scope || task.done) return false;
    for (let current = task; current; current = current.parent) {
      if (current.detached) return false;
    }
    return true;
  }
  async function awaitScopeChildren(scope) {
    for (;;) {
      const children = [...fiberTasks.values()].filter(task => ownedByScope(task, scope));
      if (!children.length) return;
      await Promise.all(children.map(task => task.promise));
    }
  }
  function detachedFailure(task) {
    if (!task.failed || task.cancelled || task.endedInvocation || disposed || fatalError) return;
    fatalError = task.error instanceof Error ? task.error : new Error("Detached Beskid fiber panicked", { cause: task.error });
    for (const other of fiberTasks.values()) other.controller.abort(fatalError);
    for (const controller of pending) controller.abort(fatalError);
    synchronization.dispose();
    globalThis.reportError?.(fatalError);
  }
  const fiberHandlers = {
    fiber_spawn(entry, environment) {
      const key = Number(entry);
      check(Number.isSafeInteger(key) && key > 0 && BigInt(key) === entry, "Invalid fiber entry");
      const run = wasmExports[`__beskid_fiber_entry_${key}`];
      check(typeof run === "function", "Unknown fiber entry");
      const scope = scopes.get();
      check(scope !== null, "Fiber spawn requires an active scope");
      check(nextFiber <= signedI64Max, "Fiber handle space exhausted");
      const id = nextFiber++;
      const parent = fiberTasks.get(fiberContext?.get());
      const task = { scope, parent, detached: false, consumed: false, result: null, hint: 0, stack: 0n, overflow: undefined,
        failed: false, done: false, cancelled: false, reason: 0n, canceler: 0n, controller: new AbortController(), error: undefined };
      task.promise = Promise.resolve().then(() => {
        requireActive();
        operationSignal(scope, id)?.throwIfAborted();
        return within(scope, () => WebAssembly.promising(run)(environment), id);
      }).then(result => {
        task.hint = Number(result);
        if (result !== 0n) task.result = within(scope, () => references.get(result), id);
      }).catch(error => {
        task.failed = true;
        task.error = error;
        task.endedInvocation = composition.isExit(error, scope);
        if (error instanceof FiberStackOverflow) task.overflow = error;
      }).finally(() => {
        task.done = true;
        task.scope = undefined;
        synchronization.release(id);
        if (fiberContext?.get() === id) {
          if (scopes.get() === scope) scopes.set(null);
          fiberContext.set(0n);
        }
      });
      fiberTasks.set(id, task);
      return id;
    },
    __beskid_fiber_stack_enter(bytes) {
      const id = fiberContext?.get() ?? 0n;
      if (id === 0n) return;
      const task = fiberTasks.get(id);
      check(task && bytes > 0n, "Invalid fiber stack reservation");
      const requested = task.stack + bytes;
      if (requested > fiberStackLimit) throw new FiberStackOverflow(fiberStackLimit, requested);
      task.stack = requested;
    },
    __beskid_fiber_stack_leave(bytes) {
      const id = fiberContext?.get() ?? 0n;
      if (id === 0n) return;
      const task = fiberTasks.get(id);
      check(task && bytes > 0n && task.stack >= bytes, "Invalid fiber stack release");
      task.stack -= bytes;
    },
    fiber_cancel(id, reason) {
      const task = fiberTask(id);
      if (!task.done && !task.cancelled) {
        task.cancelled = true;
        task.reason = reason;
        const current = fiberContext?.get() ?? 0n;
        task.canceler = current === 0n ? -1n : current;
        task.controller.abort(new DOMException("Fiber cancelled", "AbortError"));
      }
      return 1;
    },
    fiber_detach(id) {
      const task = fiberTask(id);
      task.consumed = true;
      task.detached = true;
      void task.promise.then(() => { fiberTasks.delete(id); detachedFailure(task); });
    },
    async fiber_join_status(id) { const task = fiberTask(id); await task.promise; return task.cancelled ? 1 : task.overflow ? 3 : !task.failed ? 0 : 2; },
    fiber_join_value(id) {
      const task = fiberTask(id);
      check(!task.failed && task.result !== null, "Fiber has no completed value");
      const result = references.intern(task.result, task.hint);
      task.consumed = true;
      fiberTasks.delete(id);
      return result;
    },
    fiber_join_detail(id, field) { const task = fiberTask(id); return field === 0n ? task.reason : field === 1n ? task.canceler : field === 2n ? task.overflow?.limit ?? 0n : field === 3n ? task.overflow?.requested ?? 0n : 0n; },
    fiber_join_message(id) { const task = fiberTask(id); return String(task.error?.message ?? task.error ?? "fiber failed"); },
    fiber_join_error_finish(id) { const task = fiberTask(id); task.consumed = true; fiberTasks.delete(id); return 1; },
  };
  for (const service of profile.services) {
    const { id, module, name, maySuspend, version } = service;
    const override = bindings.get(name);
    const clock = bindings.get(monotonicClock) ?? methods.get(monotonicClock).invoke;
    const method = methods.get(name);
    check(!method.requiresAdapter || override, `Beskid browser service ${id} requires an explicit adapter for ${name}`);
    if (method.networkOperation) {
      const prepare = args => {
        requireActive();
        return network.prepare(method, args, networkTransfer);
      };
      imports[module][name] = maySuspend ? new WebAssembly.Suspending(async (...args) => {
        const scope = scopes.get();
        const request = prepare(args);
        scopes.set(null);
        const response = await withAbort(request.invoke, [], pending, operationSignal(scope, fiberContext?.get()));
        requireActive();
        return within(scope, () => checkedResult(request.complete(response), service));
      }) : (...args) => {
        const request = prepare(args);
        return checkedResult(request.complete(request.invoke()), service);
      };
      if (override || !capabilities[id]) capabilities[id] = Object.freeze({ version, source: override ? "adapter" : "standard" });
      continue;
    }
    if (method.synchronization) {
      const invoke = (...args) => {
        requireActive();
        const scope = scopes?.get();
        const fiber = fiberContext?.get() ?? 0n;
        const signal = operationSignal(scope, fiber);
        const context = synchronization.context(fiber === 0n ? scope : fiber, fiber, signal);
        const result = synchronization.methods[name](context, ...args);
        if (result instanceof Promise) {
          scopes?.set(null);
          return result.then(value => {
            requireActive();
            if (composition.isExit(signal?.reason, scope)) throw signal.reason;
            return checkedResult(value, service);
          });
        }
        return checkedResult(result, service);
      };
      imports[module][name] = maySuspend ? new WebAssembly.Suspending(invoke) : invoke;
      if (!capabilities[id]) capabilities[id] = Object.freeze({ version, source: "standard" });
      continue;
    }
    const invoke = override ?? defaults[name] ?? fiberHandlers[name] ?? composition.methods[name] ?? (method.operation ? protocol.methods[method.operation] :
      name === sleepImport ? (deadline, signal) => sleepUntil(deadline, signal, clock) : method.invoke);
    const decode = args => {
      for (const index of method.strings ?? (method.stringArg === undefined ? [] : [method.stringArg])) {
        args[index] = strings.copyOut(args[index]);
      }
      for (const index of method.bytes ?? []) {
        args[index] = bytes.copyOut(args[index]);
      }
      return args;
    };
    const checkText = value => {
      check(typeof value === "string" && value.isWellFormed(), "Beskid browser service returned invalid Unicode");
      return value;
    };
    const binding = maySuspend ? new WebAssembly.Suspending(async (...args) => {
      requireActive();
      const scope = scopes?.get();
      const fiberSignal = operationSignal(scope, fiberContext?.get());
      const output = method.readText ? args.pop() : null;
      const buffer = method.readBytes ? Number(args[1]) : undefined;
      if (method.readBytes) {
        const capacity = wasmExports.array_i8_len(buffer) >>> 0;
        check(args[2] >= 0n && args[2] <= BigInt(capacity), "Invalid byte stream read limit");
        args.splice(1, 1);
      }
      decode(args);
      scopes?.set(null);
      const result = await withAbort(invoke, args, pending, fiberSignal);
      requireActive();
      if (method.readBytes) {
        if (typeof result === "bigint" && result < 0n) return checkedResult(result, service);
        check(result instanceof Uint8Array && BigInt(result.length) <= args[1], "Invalid byte stream read result");
        within(scope, () => {
          for (let i = 0; i < result.length; i++) wasmExports.array_i8_set(buffer, i, result[i]);
        });
        return BigInt(result.length);
      }
      if (method.readText) {
        const status = checkedResult(result?.status, service);
        if (status === 0) within(scope, () => wasmExports.browser_string_assign(Number(output), Number(strings.copyIn(checkText(result.text)))));
        return status;
      }
      return checkedResult(result, service);
    }) : (...args) => {
      requireActive();
      const destination = method.writeBytes === undefined ? undefined : args[method.writeBytes];
      const result = invoke(...decode(args));
      if (name === "syscall_write_bytes") {
        check(typeof result === "bigint" && result <= args[2], "Invalid byte stream write count");
      }
      if (name === "beskid_rt_v5_args_count") {
        check(typeof result === "bigint" && result >= 0n, "Invalid browser argument count");
      }
      if (name === "tty_winsize") {
        check(typeof result === "bigint" && (result === 0n ||
          (result >= 65536n && result <= 0xffffffffn && (result & 0xffffn) !== 0n)),
        "Invalid browser terminal dimensions");
      }
      if (destination !== undefined && result === 0n) {
        const buffer = args[method.writeBytes];
        for (let i = 0; i < buffer.length; i++) wasmExports.array_i8_set(Number(destination), i, buffer[i]);
      }
      return checkedResult(method.stringResult ? strings.copyIn(checkText(result)) : result, service);
    };
    imports[module][name] = binding;
    capabilities[id] = Object.freeze({ version, source: override ? "adapter" : "standard" });
  }

  const instance = new WebAssembly.Instance(module, imports);
  const exports = Object.create(null);
  const wasmExports = instance.exports;
  if (profile.internalExports.includes(stringHelpers[0])) strings = stringTransfer(wasmExports);
  const refs = managedTransfer(wasmExports);
  const bytes = byteTransfer(wasmExports);
  const networkTransfer = {
    copyString: handle => strings.copyOut(handle),
    copyBytes: handle => bytes.copyOut(handle),
    byteLength: handle => wasmExports.array_i8_len(Number(handle)) >>> 0,
    wordLength: handle => wasmExports.array_i64_len(Number(handle)) >>> 0,
    writeWord: (handle, value) => wasmExports.array_i64_set(Number(handle), 0, value),
    copyByteRange(handle, offset, count) {
      const data = new Uint8Array(count);
      for (let index = 0; index < count; index++) data[index] = wasmExports.array_i8_get(Number(handle), index + offset);
      return data;
    },
    writeBytes(handle, offset, data) {
      for (let index = 0; index < data.length; index++) wasmExports.array_i8_set(Number(handle), index + offset, data[index]);
    },
  };
  const scopes = profile.managedScopes ? {
    get: wasmExports[scopeGet],
    set: wasmExports[scopeSet],
    create: wasmExports[scopeNew],
  } : undefined;
  const fiberContext = profile.internalExports.includes(fiberGet) ? { get: wasmExports[fiberGet], set: wasmExports[fiberSet] } : undefined;
  const within = (scope, run, fiber = fiberContext?.get() ?? 0n) => {
    if (!scopes) return run();
    const previous = scopes.get();
    const previousFiber = fiberContext?.get();
    scopes.set(scope);
    fiberContext?.set(fiber);
    try {
      return run();
    } finally {
      scopes.set(previous);
      if (fiberContext) fiberContext.set(previousFiber);
    }
  };
  const runDisposal = (scope, registration, maySuspend) => within(scope, () => {
    const { invoke, args } = composition.invoke(registration);
    return (maySuspend ? WebAssembly.promising(invoke) : invoke)(...args);
  }, 0n);
  function settleComposition(scope, result, reason, failures) {
    if (failures.length) throw new AggregateError([reason, ...failures], "Beskid composition disposal failed");
    if (composition.isExit(reason, scope) && result === "i32") return reason.code;
    throw reason;
  }
  for (const { name, parameters, results, maySuspend, awaitChildren } of profile.exports) {
    const wasm = instance.exports[name];
    const invoke = maySuspend ? WebAssembly.promising(wasm) : wasm;
    const run = args => {
      requireActive();
      check(args.length === parameters.length, `Beskid browser export ${name} expects ${parameters.length} arguments`);
      return invoke(...parameters.map(({ type, typeId }, index) => {
        if (type === "managed") return refs.copyIn(args[index], typeId);
        if (type === "bytes") return bytes.copyIn(args[index]);
        return valueForWasm(type, args[index], name, strings);
      }));
    };
    const resultType = results[0]?.type;
    const decode = result => {
      if (resultType === "managed") return refs.copyOut(result, results[0].typeId);
      if (resultType === "bytes") return bytes.copyOut(result);
      return valueFromWasm(resultType, result, strings);
    };
    exports[name] = maySuspend ? async (...args) => {
      requireActive();
      const scope = scopes?.create();
      try {
        const result = await within(scope, () => run(args), 0n);
        if (awaitChildren) await awaitScopeChildren(scope);
        composition.signal(scope)?.throwIfAborted();
        requireActive();
        return within(scope, () => decode(result), 0n);
      } catch (error) {
        const failures = [];
        for (const registration of composition.remaining(scope)) {
          try { await runDisposal(scope, registration, true); } catch (failure) {
            if (composition.isExit(error, scope) && composition.isExit(failure, scope)) error = failure;
            else failures.push(failure);
            composition.clearExit(scope);
          }
        }
        return settleComposition(scope, resultType, error, failures);
      } finally {
        composition.clearExit(scope);
        synchronization.release(scope);
        // A resumed Wasm call can leave its own scope installed at return.
        if (scopes && scopes.get() === scope) scopes.set(null);
      }
    } : (...args) => {
      requireActive();
      const scope = scopes?.create();
      try {
        return within(scope, () => {
          const result = run(args);
          requireActive();
          return decode(result);
        }, 0n);
      } catch (error) {
        const failures = [];
        for (const registration of composition.remaining(scope)) {
          try { runDisposal(scope, registration, false); } catch (failure) {
            if (composition.isExit(error, scope) && composition.isExit(failure, scope)) error = failure;
            else failures.push(failure);
            composition.clearExit(scope);
          }
        }
        return settleComposition(scope, resultType, error, failures);
      } finally {
        composition.clearExit(scope);
        synchronization.release(scope);
      }
    };
  }

  return Object.freeze({
    exports: Object.freeze(exports),
    capabilities: Object.freeze(capabilities),
    dispose() {
      if (disposed) return;
      disposed = true;
      refs.clear();
      fiberTasks.clear();
      synchronization.dispose();
      composition.dispose();
      environment.clear();
      for (const controller of pending) {
        controller.abort(new DOMException("Beskid browser instance is disposed", "AbortError"));
      }
      const errors = [];
      for (const close of teardown) {
        try { close(); } catch (error) { errors.push(error); }
      }
      if (errors.length) throw new AggregateError(errors, "Disposing Beskid browser adapters failed");
    },
  });
}
