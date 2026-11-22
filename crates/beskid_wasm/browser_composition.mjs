class BeskidProgramExit extends Error {
  constructor(code) {
    super(`Beskid invocation exited with status ${code}`);
    this.name = "BeskidProgramExit";
    Object.defineProperty(this, "code", { value: code, enumerable: true });
  }
}

function createBrowserComposition(scope, references, hook, canExit, cancelFibers) {
  const containers = new Map();
  const owners = new Map();
  const exits = new WeakMap();
  const requests = new WeakMap();
  let next = 1n;

  const container = id => {
    const state = containers.get(id);
    check(state && state.owner === scope(), "Invalid Beskid composition container");
    return state;
  };
  const exit = owner => {
    if (!canExit || !owner) return undefined;
    let state = exits.get(owner);
    if (!state) {
      state = new AbortController();
      exits.set(owner, state);
    }
    return state;
  };

  const methods = {
    composition_container_create(slots) {
      const owner = scope();
      check(owner && !owners.has(owner) && slots >= 0n && next <= signedI64Max, "Invalid Beskid composition launch");
      const id = next++;
      owners.set(owner, id);
      containers.set(id, { owner, slotCount: slots, slots: new Map(), frames: [{ id: 0n, parent: 0n, dispose: [] }], active: false });
      return id;
    },
    composition_slot_store(id, slot, handle) {
      const state = container(id);
      if (state.active || slot < 0n || slot >= state.slotCount || state.slots.has(slot) || handle === 0n) return 0;
      const reference = references.get(handle);
      if (reference === null) return 0;
      state.slots.set(slot, reference);
      return 1;
    },
    composition_launch(id) {
      const state = container(id);
      if (state.active || BigInt(state.slots.size) !== state.slotCount) return 0;
      state.active = true;
      return 1;
    },
    composition_shutdown(id) {
      const state = container(id);
      state.active = false;
      state.slots.clear();
      state.frames.length = 0;
    },
    composition_container_drop(id) {
      const state = container(id);
      owners.delete(state.owner);
      containers.delete(id);
    },
    composition_scope_enter(id, scopeId, parent) {
      const state = container(id);
      const frame = state.frames.at(-1);
      check(state.active && scopeId !== 0n && scopeId !== parent &&
        (frame.id === parent || (frame.id === scopeId && frame.parent === parent)), "Invalid Beskid composition scope");
      state.frames.push({ id: scopeId, parent, dispose: [] });
    },
    composition_scope_leave() {
      const state = container(owners.get(scope()));
      check(state.frames.length > 1 && state.frames.at(-1).dispose.length === 0, "Invalid Beskid composition scope exit");
      state.frames.pop();
    },
    composition_exit_register(id, entry, count, ...handles) {
      const state = container(id);
      check(entry > 0n && count >= 0n && count <= BigInt(handles.length) && typeof hook(entry, count) === "function", "Invalid Beskid composition disposal hook");
      const arguments_ = handles.slice(0, Number(count)).map(handle => {
        check(handle !== 0n, "Missing Beskid composition hook argument");
        const reference = references.get(handle);
        check(reference !== null, "Invalid Beskid composition hook argument");
        return { reference, handle };
      });
      state.frames.at(-1).dispose.push({ entry, arguments_ });
    },
    composition_exit_pop(id) {
      const state = container(id);
      check(state.frames.at(-1).dispose.pop(), "Missing Beskid composition disposal hook");
    },
    composition_exit(code) {
      const request = new BeskidProgramExit(code);
      const owner = scope();
      const controller = exit(owner);
      check(controller, "Beskid program exit requires an active invocation");
      requests.set(request, new WeakRef(owner));
      controller.abort(request);
      cancelFibers(owner, request);
      throw request;
    },
  };

  function remaining(owner) {
    exits.delete(owner);
    const id = owners.get(owner);
    if (id === undefined) return [];
    const state = containers.get(id);
    owners.delete(owner);
    containers.delete(id);
    return state.frames.reverse().flatMap(frame => frame.dispose.reverse());
  }

  function invoke({ entry, arguments_ }) {
    return { invoke: hook(entry, arguments_.length), args: arguments_.map(({ reference, handle }) => references.intern(reference, Number(handle))) };
  }

  return {
    methods, remaining, invoke,
    signal: owner => exit(owner)?.signal,
    isExit: (reason, owner) => owner != null && requests.get(reason)?.deref() === owner,
    clearExit: owner => exits.delete(owner),
    dispose() { owners.clear(); containers.clear(); },
  };
}
