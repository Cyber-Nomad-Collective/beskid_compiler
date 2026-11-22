const networkPrefix = "beskid_rt_v5_network_";

const networkStatus = Object.freeze({
  invalidAddress: 7,
  notConnected: 10,
  denied: 11,
  timedOut: 12,
  cancelled: 13,
  closed: 14,
  busy: 15,
  cleanupFailed: 16,
  unsupported: 17,
});

function checkedNetworkStatus(code) {
  check(inRange(code, 0, 18), "Invalid Beskid networking adapter status");
  return code;
}

function checkedNetworkAddress(value) {
  check(value instanceof Uint8Array && value.length === 19 && (value[0] === 4 || value[0] === 6),
    "Invalid Beskid networking adapter address");
  return value.slice();
}

function networkFailure(method, status) {
  if (method.results[0] === "i32") return status;
  if (method.results[0] === "i64") return -BigInt(status);
}

/** A provider operates on copied byte buffers and JavaScript resources. The guest
 * receives fresh instance-local words and never sees a provider's own handles. */
function createBrowserNetwork(bindings) {
  const provider = action => bindings.get(networkPrefix + action);
  const sockets = new Map();
  const resolutions = new Map();
  const owned = new WeakSet();
  let next = 1n;
  const closeProvider = provider("close");
  check(!(provider("open") || provider("accept")) || closeProvider,
    "Beskid networking adapter requires close when it creates a socket");

  function retire(socket) {
    for (const controller of socket.pending.values()) controller.abort();
    owned.delete(socket.resource);
    try { return checkedNetworkStatus(closeProvider(socket.resource)); }
    catch { return networkStatus.cleanupFailed; }
  }

  function register(kind, resource) {
    check(resource !== null && (typeof resource === "object" || typeof resource === "function") && !owned.has(resource),
      "Invalid or duplicate Beskid networking adapter socket");
    const key = next++;
    owned.add(resource);
    sockets.set(key, { resource, kind, pending: new Map() });
    return key;
  }

  const outcome = value => ({ invoke: () => value, complete: value => value });
  const result = (invoke, complete) => ({ invoke, complete });
  const output = (network, handle) => {
    check(network.wordLength(handle) >= 1, "Invalid Beskid networking service handle destination");
    return value => network.writeWord(handle, value);
  };

  function resourceResult(value, kind, setHandle) {
    const status = checkedNetworkStatus(value?.status);
    if (status === 0) setHandle(register(kind, value.resource));
    return status;
  }

  function cleanupLateResource(value) {
    if (value?.status === 0 && value.resource && !owned.has(value.resource)) {
      closeProvider(value.resource);
    }
  }

  function abortStatus(error) {
    return { NotAllowedError: networkStatus.denied, SecurityError: networkStatus.denied,
      TimeoutError: networkStatus.timedOut, AbortError: networkStatus.cancelled }[error?.name];
  }

  function invokeAsync(call, signal, closeLate, cancelled) {
    return Promise.resolve().then(() => {
      if (signal.aborted) return cancelled;
      return call();
    }).then(value => {
      if (!signal.aborted) return value;
      if (closeLate) cleanupLateResource(value);
      return cancelled;
    }, error => {
      if (signal.aborted) return cancelled;
      const status = abortStatus(error);
      if (status === undefined) throw error;
      return { status };
    });
  }

  function invokeSocket(socket, activity, call, outer, closesLate) {
    if (socket.pending.has(activity)) return { status: networkStatus.busy };
    const controller = new AbortController();
    socket.pending.set(activity, controller);
    const signal = AbortSignal.any([outer, controller.signal]);
    const closed = { status: networkStatus.closed };
    const interrupted = Promise.withResolvers();
    const onAbort = () => interrupted.resolve(closed);
    signal.addEventListener("abort", onAbort, { once: true });
    if (signal.aborted) onAbort();
    return Promise.race([
      invokeAsync(() => call(signal), signal, closesLate, closed),
      interrupted.promise,
    ]).finally(() => {
      signal.removeEventListener("abort", onAbort);
      socket.pending.delete(activity);
    });
  }

  function prepare(method, args, transfer) {
    const op = method.networkOperation;
    const operation = provider(op);
    const failure = code => outcome(networkFailure(method, code));
    const cancelled = { status: networkStatus.cancelled };
    const closed = { status: networkStatus.closed };
    if (op === "dns_count" || op === "dns_address" || op === "dns_release") {
      const [key, index, destination] = args;
      const addresses = resolutions.get(key);
      if (op === "dns_release") {
        return result(() => resolutions.delete(key), () => undefined);
      }
      if (!provider("dns_resolve")) return failure(networkStatus.unsupported);
      if (!addresses) return failure(networkStatus.closed);
      if (op === "dns_count") return outcome(BigInt(addresses.length));
      if (index < 0n || index >= BigInt(addresses.length)) return failure(networkStatus.invalidAddress);
      check(transfer.byteLength(destination) >= 19, "Invalid Beskid DNS address destination");
      return result(() => addresses[Number(index)], address => {
        transfer.writeBytes(destination, 0, address);
        return 0;
      });
    }
    if (!operation) return failure(networkStatus.unsupported);
    if (op === "open") {
      const [kind, address, options, backlog, deadline, destination] = args;
      if (kind < 1n || kind > 3n || options < 0n || options > 3n || backlog < 0n || backlog > 0x7fffffffn) {
        return failure(networkStatus.invalidAddress);
      }
      const wire = checkedNetworkAddress(transfer.copyBytes(address));
      const setHandle = output(transfer, destination);
      return result(
        signal => invokeAsync(() => operation({ kind, address: wire, options, backlog, deadline }, signal),
          signal, true, cancelled),
        value => resourceResult(value, kind, setHandle),
      );
    }
    if (op === "dns_resolve") {
      const [host, port, family, deadline, destination] = args;
      if (port < 1n || port > 65535n || ![0n, 4n, 6n].includes(family)) return failure(networkStatus.invalidAddress);
      const hostName = transfer.copyString(host);
      const setHandle = output(transfer, destination);
      return result(
        signal => invokeAsync(() => operation(hostName, port, family, deadline, signal), signal, false, cancelled),
        value => {
          const status = checkedNetworkStatus(value?.status);
          if (status === 0) {
            check(Array.isArray(value.addresses), "Invalid Beskid networking adapter DNS result");
            const addresses = value.addresses.map(checkedNetworkAddress);
            for (const address of addresses) {
              check((family === 0n || family === BigInt(address[0])) && port === BigInt(address[1] * 256 + address[2]),
                "Beskid networking adapter DNS result does not match its request");
            }
            const key = next++;
            resolutions.set(key, addresses);
            setHandle(key);
          }
          return status;
        },
      );
    }
    const [key, a, b, c, d] = args;
    const socket = sockets.get(key);
    if (!socket) return failure(networkStatus.closed);
    const { kind, resource } = socket;
    if (op === "close") {
      return result(() => {
        sockets.delete(key);
        return retire(socket);
      }, value => value);
    }
    const syncStatus = (...parameters) => result(() => operation(resource, ...parameters), checkedNetworkStatus);
    const invoke = (activity, callback, createsSocket = false) =>
      signal => invokeSocket(socket, activity, callback, signal, createsSocket);
    const completeCount = (value, maximum) => {
      const status = checkedNetworkStatus(value?.status);
      if (status !== 0) return -BigInt(status);
      check(typeof value.count === "bigint" && value.count >= 0n && value.count <= maximum,
        "Invalid Beskid networking adapter write count");
      return value.count;
    };
    if (op === "accept") {
      if (kind !== 1n) return failure(networkStatus.notConnected);
      const setHandle = output(transfer, b);
      return result(invoke("accept", signal => operation(resource, a, signal), true),
        value => resourceResult(value, 2n, setHandle));
    }
    if (op === "options") {
      return result(() => operation(resource), value => {
        const status = checkedNetworkStatus(value?.status);
        if (status !== 0) return -BigInt(status);
        check(typeof value.options === "bigint" && value.options >= 0n && value.options <= 3n,
          "Invalid Beskid networking adapter options");
        return value.options;
      });
    }
    if (op === "set_options") {
      return a >= 0n && a <= 3n ? syncStatus(a) : failure(networkStatus.invalidAddress);
    }
    if (op === "set_deadlines") return syncStatus(a, b);
    if (op === "shutdown_write") return kind === 2n ? syncStatus() : failure(networkStatus.notConnected);
    if (op === "address") {
      check(transfer.byteLength(b) >= 19, "Invalid Beskid networking service address destination");
      return result(() => operation(resource, a !== 0), value => {
        const status = checkedNetworkStatus(value?.status);
        if (status === 0) transfer.writeBytes(b, 0, checkedNetworkAddress(value.address));
        return status;
      });
    }
    if (op === "udp_connect") {
      if (kind !== 3n) return failure(networkStatus.notConnected);
      const address = checkedNetworkAddress(transfer.copyBytes(a));
      return result(invoke("connect", signal => Promise.resolve(operation(resource, address, signal)).then(status => ({ status }))),
        value => checkedNetworkStatus(value?.status));
    }
    if (op === "read" || op === "write") {
      if (kind !== 2n) return failure(networkStatus.notConnected);
      if (b < 0n || c < 0n || b + c > BigInt(transfer.byteLength(a))) return failure(networkStatus.invalidAddress);
      if (op === "write") {
        const bytes = transfer.copyByteRange(a, Number(b), Number(c));
        return result(invoke("write", signal => operation(resource, bytes, signal)),
          value => completeCount(value, c));
      }
      return result(invoke("read", signal => operation(resource, c, d, signal)), value => {
        const status = checkedNetworkStatus(value?.status);
        if (status !== 0) return -BigInt(status);
        check(value.bytes instanceof Uint8Array && BigInt(value.bytes.length) <= c, "Invalid Beskid networking adapter read result");
        transfer.writeBytes(a, Number(b), value.bytes);
        return BigInt(value.bytes.length);
      });
    }
    if (op === "receive") {
      if (kind !== 3n) return failure(networkStatus.notConnected);
      const maximum = transfer.byteLength(a);
      check(transfer.byteLength(b) >= 20, "Invalid Beskid datagram metadata destination");
      return result(invoke("read", signal => operation(resource, BigInt(maximum), c !== 0, d, signal)), value => {
        const status = checkedNetworkStatus(value?.status);
        if (status !== 0) return -BigInt(status);
        check(value.bytes instanceof Uint8Array && value.bytes.length <= maximum && typeof value.truncated === "boolean",
          "Invalid Beskid networking adapter datagram result");
        const address = checkedNetworkAddress(value.address);
        const metadata = new Uint8Array(20);
        metadata.set(address);
        metadata[19] = Number(value.truncated);
        transfer.writeBytes(b, 0, metadata);
        transfer.writeBytes(a, 0, value.bytes);
        return BigInt(value.bytes.length);
      });
    }
    if (op === "send") {
      if (kind !== 3n) return failure(networkStatus.notConnected);
      const bytes = transfer.copyBytes(a);
      const address = c !== 0 ? null : checkedNetworkAddress(transfer.copyBytes(b));
      return result(invoke("write", signal => operation(resource, bytes, address, d, signal)),
        value => completeCount(value, BigInt(bytes.length)));
    }
    throw new TypeError("Unsupported Beskid networking operation");
  }

  return {
    prepare,
    dispose() {
      resolutions.clear();
      let failed = false;
      for (const [key, socket] of sockets) {
        sockets.delete(key);
        if (retire(socket) !== 0) failed = true;
      }
      if (failed) throw new Error("A Beskid networking adapter failed to close a socket");
    },
  };
}
