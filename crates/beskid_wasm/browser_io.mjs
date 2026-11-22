// These negative results cross the scalar boundary. Positive values are
// instance-local resource handles. Zero is used for successful unit results.
const protocolError = Object.freeze({
  invalidUrl: -1n,
  scheme: -2n,
  unsupported: -3n,
  transport: -4n,
  timeout: -5n,
  limit: -6n,
  closed: -7n,
  invalidMessage: -8n,
  busy: -9n,
  headerCount: -10n,
  headerSize: -11n,
});
const protocolEncoder = new TextEncoder();

function protocolUrl(value, schemes) {
  let url;
  try { url = new URL(value); }
  catch { return protocolError.invalidUrl; }
  if (!schemes.includes(url.protocol)) return protocolError.scheme;
  if (url.username || url.password) return protocolError.invalidUrl;
  return url;
}

function protocolTimeout(millis) {
  return millis > 0n ? AbortSignal.timeout(Number(millis > 2_147_483_647n ? 2_147_483_647n : millis)) : undefined;
}

function protocolSignal(cancellation, deadline) {
  return deadline ? AbortSignal.any([cancellation, deadline]) : cancellation;
}

function parseHeaderLines(lines) {
  const headers = new Headers();
  if (lines === "") return headers;
  for (const line of lines.split("\n")) {
    const colon = line.indexOf(":");
    if (colon <= 0 || line.includes("\r")) throw new TypeError("Invalid HTTP request header");
    headers.append(line.slice(0, colon), line.slice(colon + 1));
  }
  return headers;
}

async function readFetchBody(response, limit) {
  if (!response.body) return new Uint8Array();
  const reader = response.body.getReader();
  const chunks = [];
  try {
    let length = 0;
    while (true) {
      const { value, done } = await reader.read();
      if (done) break;
      length += value.byteLength;
      if (BigInt(length) > limit || length > 0x7fffffff) {
        await reader.cancel();
        return protocolError.limit;
      }
      chunks.push(value);
    }
    const bytes = new Uint8Array(length);
    let offset = 0;
    for (const chunk of chunks) {
      bytes.set(chunk, offset);
      offset += chunk.byteLength;
    }
    return bytes;
  } finally {
    reader.releaseLock();
  }
}

function createBrowserIo() {
  const slots = new Map();
  let next = 1n;
  const store = (kind, value) => {
    const key = next++;
    slots.set(key, { kind, value });
    return key;
  };
  const get = (key, kind) => {
    const found = slots.get(key);
    if (found?.kind !== kind) throw new WebAssembly.RuntimeError("Invalid Beskid browser protocol resource");
    return found.value;
  };
  const drop = (key, kind) => {
    const value = get(key, kind);
    slots.delete(key);
    return value;
  };

  async function httpFetch(target, method, lines, body, timeout, maxHeaders, maxHeaderBytes, maxBodyBytes, signal) {
    const url = protocolUrl(target, ["http:", "https:"]);
    if (typeof url === "bigint") return url;
    if (maxHeaders < 0n || maxHeaderBytes < 0n || maxBodyBytes < 0n) return protocolError.unsupported;
    let request;
    const deadline = protocolTimeout(timeout);
    try {
      const headers = parseHeaderLines(lines);
      request = new Request(url, {
        method, headers, body: body.length ? body : undefined,
        redirect: "manual", signal: protocolSignal(signal, deadline),
      });
      // Fetch may silently discard headers controlled by the user agent.
      for (const [name, value] of headers) {
        if (request.headers.get(name) !== value) return protocolError.unsupported;
      }
    } catch {
      return protocolError.unsupported;
    }
    try {
      const response = await fetch(request);
      if (response.type === "opaqueredirect" || response.type === "opaque") return protocolError.unsupported;
      const headers = [...response.headers];
      let headerBytes = 0;
      for (const [name, value] of headers) headerBytes += protocolEncoder.encode(name + ": " + value + "\r\n").length;
      if (BigInt(headers.length) > maxHeaders) {
        await response.body?.cancel();
        return protocolError.headerCount;
      }
      if (BigInt(headerBytes) > maxHeaderBytes) {
        await response.body?.cancel();
        return protocolError.headerSize;
      }
      const bytes = await readFetchBody(response, maxBodyBytes);
      signal.throwIfAborted();
      if (typeof bytes === "bigint") return bytes;
      return store("http", { headers, bytes, code: response.status, reason: response.statusText });
    } catch {
      signal.throwIfAborted();
      return deadline?.aborted ? protocolError.timeout : protocolError.transport;
    }
  }

  function timeoutResult(signal) {
    signal.throwIfAborted();
    return protocolError.timeout;
  }

  function websocketEvent(socket, eventNames, signal) {
    return new Promise(resolve => {
      const done = event => {
        for (const name of eventNames) socket.removeEventListener(name, done);
        signal.removeEventListener("abort", done);
        resolve(event.type);
      };
      if (signal.aborted) {
        resolve("abort");
        return;
      }
      for (const name of eventNames) socket.addEventListener(name, done);
      signal.addEventListener("abort", done, { once: true });
    });
  }

  async function wsOpen(target, protocols, maxBytes, handshakeTimeout, closeTimeout, readTimeout, signal) {
    const url = protocolUrl(target, ["ws:", "wss:"]);
    if (typeof url === "bigint") return url;
    if (url.href.includes("#")) return protocolError.invalidUrl;
    if (maxBytes <= 0n || maxBytes > 0x7fffffffn) return protocolError.unsupported;
    let socket;
    try { socket = new WebSocket(url, protocols ? protocols.split("\n") : []); }
    catch { return protocolError.unsupported; }
    const connection = {
      socket, maxBytes: Number(maxBytes), closeTimeout, readTimeout,
      maxQueueBytes: Math.max(Number(maxBytes), 4 * 1024 * 1024),
      queue: [], queuedBytes: 0, waiter: undefined, failure: undefined, close: undefined,
      resource: url.pathname + url.search,
      origin: globalThis.location.origin,
    };
    socket.binaryType = "arraybuffer";
    const wake = () => connection.waiter?.();
    socket.addEventListener("message", event => {
      if (connection.failure !== undefined) return;
      const data = typeof event.data === "string" ? event.data : new Uint8Array(event.data);
      const size = typeof data === "string" ? protocolEncoder.encode(data).byteLength : data.byteLength;
      // Bound the queued payload as well as the size of each individual message.
      if (size > connection.maxBytes || connection.queuedBytes + size > connection.maxQueueBytes || connection.queue.length >= 4096) {
        connection.failure = protocolError.limit;
        socket.close(4000, "message limit");
      } else {
        connection.queue.push({ kind: typeof data === "string" ? 1n : 2n, data, size });
        connection.queuedBytes += size;
      }
      wake();
    });
    socket.addEventListener("close", event => {
      connection.close = { kind: 3n, code: BigInt(event.code), data: event.reason };
      connection.closeCode = BigInt(event.code);
      wake();
    });
    const open = await websocketEvent(socket, ["open", "error", "close"], protocolSignal(signal, protocolTimeout(handshakeTimeout)));
    if (open === "open" && !signal.aborted) return store("socket", connection);
    socket.close();
    if (open === "abort" || signal.aborted) return timeoutResult(signal);
    return protocolError.transport;
  }

  function wsSend(key, data) {
    const connection = get(key, "socket");
    if (connection.socket.readyState !== WebSocket.OPEN) return protocolError.closed;
    const size = typeof data === "string" ? protocolEncoder.encode(data).byteLength : data.byteLength;
    if (size > connection.maxBytes || size + connection.socket.bufferedAmount > connection.maxQueueBytes) return protocolError.limit;
    connection.socket.send(data);
    return 0n;
  }

  function wsWaitMessage(connection, signal) {
    return new Promise(resolve => {
      const ready = () => {
        connection.waiter = undefined;
        signal.removeEventListener("abort", ready);
        resolve();
      };
      if (signal.aborted) {
        resolve();
        return;
      }
      connection.waiter = ready;
      signal.addEventListener("abort", ready, { once: true });
    });
  }

  async function wsReceive(key, signal) {
    const connection = get(key, "socket");
    if (connection.waiter) return protocolError.busy;
    if (!connection.queue.length && connection.failure === undefined && !connection.close) {
      const limit = protocolSignal(signal, protocolTimeout(connection.readTimeout));
      await wsWaitMessage(connection, limit);
      if (limit.aborted) return timeoutResult(signal);
    }
    signal.throwIfAborted();
    const message = connection.queue.shift();
    if (message) {
      connection.queuedBytes -= message.size;
      return store("message", message);
    }
    if (connection.failure !== undefined) return connection.failure;
    if (connection.close) {
      const close = connection.close;
      connection.close = undefined;
      connection.failure = protocolError.closed;
      return store("message", close);
    }
    return protocolError.closed;
  }

  async function wsClose(key, code, reason, signal) {
    const connection = get(key, "socket");
    const { socket } = connection;
    if (socket.readyState === WebSocket.CLOSED) return 0n;
    if (socket.readyState === WebSocket.OPEN) {
      if (code !== 1000n && (code < 3000n || code > 4999n)) return protocolError.unsupported;
      if (protocolEncoder.encode(reason).byteLength > 123) return protocolError.invalidMessage;
      socket.close(Number(code), reason);
    }
    const result = await websocketEvent(socket, ["close"], protocolSignal(signal, protocolTimeout(connection.closeTimeout)));
    return result === "abort" ? timeoutResult(signal) : 0n;
  }

  return {
    methods: {
      browser_http_fetch: httpFetch,
      browser_http_status: key => BigInt(get(key, "http").code),
      browser_http_reason: key => get(key, "http").reason,
      browser_http_header_count: key => BigInt(get(key, "http").headers.length),
      browser_http_header_name: (key, index) => get(key, "http").headers[Number(index)][0],
      browser_http_header_value: (key, index) => get(key, "http").headers[Number(index)][1],
      browser_http_body_len: key => BigInt(get(key, "http").bytes.length),
      browser_http_body_byte: (key, index) => BigInt(get(key, "http").bytes[Number(index)]),
      browser_http_drop: key => { drop(key, "http"); },
      browser_ws_open: wsOpen,
      browser_ws_send_text: wsSend,
      browser_ws_send_binary: wsSend,
      browser_ws_receive: wsReceive,
      browser_ws_message_kind: key => get(key, "message").kind,
      browser_ws_message_code: key => get(key, "message").code,
      browser_ws_message_text: key => get(key, "message").data,
      browser_ws_message_len: key => BigInt(get(key, "message").data.length),
      browser_ws_message_byte: (key, index) => BigInt(get(key, "message").data[Number(index)]),
      browser_ws_message_drop: key => { drop(key, "message"); },
      browser_ws_close: wsClose,
      browser_ws_protocol: key => get(key, "socket").socket.protocol,
      browser_ws_resource: key => get(key, "socket").resource,
      browser_ws_origin: key => get(key, "socket").origin,
      browser_ws_state: key => {
        const state = get(key, "socket").socket.readyState;
        return state === WebSocket.OPEN ? 0n : state === WebSocket.CLOSING ? 1n : 2n;
      },
      browser_ws_close_code: key => get(key, "socket").closeCode ?? 0n,
    },
    dispose() {
      for (const { kind, value } of slots.values()) {
        if (kind === "socket") value.socket.close();
      }
      slots.clear();
    },
  };
}
