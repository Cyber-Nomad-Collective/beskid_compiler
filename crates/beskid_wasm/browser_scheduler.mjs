// Cooperative synchronization over one browser instance and its Wasm GC heap.
function createBrowserSynchronization(references, yieldTask) {
  const OK = 0n;
  const CLOSED = 1n;
  const CANCELLED = 2n;
  const WOULD_BLOCK = 3n;
  const HUB_EMPTY = 4n;
  const HUB_NOT_FOUND = 6n;
  const shutdown = new AbortController();
  const channels = new Map();
  const hubs = new Map();
  const mutexes = new Map();
  const waitGroups = new Map();
  const claimed = new Map();
  const hubReceipts = new Map();
  let nextChannel = 0n;
  let nextHub = 0n;
  let nextMutex = 1n;
  let nextWaitGroup = 1n;

  function wakeFirst(waiters) {
    waiters.values().next().value?.();
  }

  function wakeAll(waiters) {
    for (const wake of waiters) wake();
  }

  function waitForChange(waiters, context) {
    return new Promise(resolve => {
      const finish = () => {
        waiters.delete(finish);
        context.signal.removeEventListener("abort", finish);
        resolve();
      };
      waiters.add(finish);
      if (context.signal.aborted) finish();
      else context.signal.addEventListener("abort", finish, { once: true });
    });
  }

  function awaitStatus(waiters, context, attempt) {
    const status = attempt();
    return status === WOULD_BLOCK ? parkUntilReady(waiters, context, attempt) : status;
  }

  async function parkUntilReady(waiters, context, attempt) {
    let status;
    do {
      await waitForChange(waiters, context);
      status = attempt();
    } while (status === WOULD_BLOCK);
    return status;
  }

  function enqueue(queue, value) {
    const node = { value, next: null };
    if (queue.back === null) queue.front = node;
    else queue.back.next = node;
    queue.back = node;
    queue.count++;
  }

  function dequeue(queue) {
    const node = queue.front;
    queue.front = node.next;
    if (queue.front === null) queue.back = null;
    queue.count--;
    return node.value;
  }

  function channelChanged(channel) {
    wakeFirst(channel.receivers);
    for (const hub of channel.hubs) wakeAll(hub.waiters);
  }

  function commitSend(channel, payload, context) {
    if (!channel) return CLOSED;
    if (context.signal.aborted) return CANCELLED;
    if (channel.closed) return CLOSED;
    if (channel.capacity !== 0n && channel.count >= channel.capacity) return WOULD_BLOCK;
    enqueue(channel, payload);
    channelChanged(channel);
    return OK;
  }

  function claimReceive(channel, context) {
    if (!channel) return CLOSED;
    if (context.signal.aborted) return CANCELLED;
    if (channel.receipt !== null) return WOULD_BLOCK;
    if (channel.front === null) return channel.closed ? CLOSED : WOULD_BLOCK;
    channel.receipt = { owner: context.owner, value: dequeue(channel) };
    if (!claimed.has(context.owner)) claimed.set(context.owner, new Set());
    claimed.get(context.owner).add(channel);
    wakeFirst(channel.senders);
    return OK;
  }

  function consume(channel, context) {
    check(channel?.receipt?.owner === context.owner, "Channel receipt belongs to another browser continuation");
    const payload = channel.receipt.value;
    channel.receipt = null;
    const receipts = claimed.get(context.owner);
    receipts.delete(channel);
    if (receipts.size === 0) claimed.delete(context.owner);
    channelChanged(channel);
    return typeof payload === "bigint" ? payload : references.intern(payload.object, payload.hint);
  }

  function payload(value, managed) {
    if (!managed || value === 0n) return value;
    return { object: references.get(value), hint: Number(value) };
  }

  function updateWaitGroup(id, delta) {
    const group = waitGroups.get(id);
    if (!group || group.count + delta < 0n) return;
    group.count = BigInt.asIntN(64, group.count + delta);
    if (group.count <= 0n) wakeAll(group.waiters);
  }

  function acquire(mutex, context) {
    if (!mutex) return CLOSED;
    if (context.signal.aborted) return CANCELLED;
    if (mutex.owner !== null) return WOULD_BLOCK;
    mutex.owner = context.fiber;
    return OK;
  }

  function changeRegistration(hub, index, channel) {
    const position = hub.entries.findIndex(entry => entry.index === index);
    if (position >= 0) {
      const old = hub.entries[position].channel;
      if (channel) hub.entries[position] = { index, channel };
      else {
        hub.entries.splice(position, 1);
        if (hub.cursor > position) hub.cursor--;
        hub.cursor = hub.entries.length ? hub.cursor % hub.entries.length : 0;
      }
      if (!hub.entries.some(entry => entry.channel === old)) old.hubs.delete(hub);
    } else if (channel) {
      hub.entries.push({ index, channel });
    } else {
      return HUB_NOT_FOUND;
    }
    channel?.hubs.add(hub);
    wakeAll(hub.waiters);
    return OK;
  }

  function recordHubReceipt(hub, owner, receipt) {
    if (!hubReceipts.has(owner)) hubReceipts.set(owner, new Map());
    hubReceipts.get(owner).set(hub, receipt);
  }

  function pollHub(hub, context) {
    if (!hub) return HUB_NOT_FOUND;
    const count = hub.entries.length;
    if (!count) return HUB_EMPTY;
    let closed = 0;
    for (let attempt = 0; attempt < count; attempt++) {
      const position = (hub.cursor + attempt) % count;
      const entry = hub.entries[position];
      const status = claimReceive(entry.channel, context);
      if (status === OK) {
        recordHubReceipt(hub, context.owner, entry);
        hub.cursor = (position + 1) % count;
        return OK;
      }
      if (status === CANCELLED) return CANCELLED;
      if (status === CLOSED) closed++;
    }
    if (closed === count) {
      recordHubReceipt(hub, context.owner, { index: hub.entries[hub.cursor].index });
      hub.cursor = (hub.cursor + 1) % count;
      return CLOSED;
    }
    return WOULD_BLOCK;
  }

  const methods = {
    isolate_current() { return 1n; },
    isolate_main() { return 1n; },
    wait_group_create() {
      if (nextWaitGroup > signedI64Max) return 0n;
      const id = nextWaitGroup++;
      waitGroups.set(id, { count: 0n, waiters: new Set() });
      return id;
    },
    wait_group_add(_context, id, delta) { updateWaitGroup(id, delta); },
    wait_group_done(_context, id) {
      if (waitGroups.get(id)?.count > 0n) updateWaitGroup(id, -1n);
    },
    wait_group_wait(context, id) {
      const group = waitGroups.get(id);
      if (!group) return 1;
      const status = awaitStatus(group.waiters, context, () => {
        if (context.signal.aborted) return CLOSED;
        return group.count <= 0n ? OK : WOULD_BLOCK;
      });
      return status instanceof Promise ? status.then(Number) : Number(status);
    },
    mutex_create() {
      if (nextMutex > signedI64Max) return 0n;
      const id = nextMutex++;
      mutexes.set(id, { owner: null, waiters: new Set() });
      return id;
    },
    mutex_lock(context, id) {
      const mutex = mutexes.get(id);
      if (!mutex) return 1;
      const status = awaitStatus(mutex.waiters, context, () => acquire(mutex, context));
      return status instanceof Promise ? status.then(Number) : Number(status);
    },
    mutex_try_lock(context, id) {
      const status = acquire(mutexes.get(id), context);
      return status === WOULD_BLOCK ? 1 : Number(status);
    },
    mutex_unlock(context, id) {
      const mutex = mutexes.get(id);
      if (mutex?.owner !== context.fiber) return;
      mutex.owner = null;
      wakeFirst(mutex.waiters);
    },
    channel_create(_context, capacity, _flags) {
      if (capacity < 0n || nextChannel > signedI64Max) return -1n;
      const id = nextChannel++;
      channels.set(id, {
        capacity, closed: false, count: 0n, front: null, back: null, receipt: null,
        senders: new Set(), receivers: new Set(), hubs: new Set(),
      });
      return id;
    },
    channel_send(context, id, value, managed) {
      const channel = channels.get(id);
      if (!channel) return CLOSED;
      const cell = payload(value, managed);
      const status = awaitStatus(channel.senders, context, () => commitSend(channel, cell, context));
      const complete = result => {
        if (result !== OK || context.fiber === 0n) return result;
        if (context.signal.aborted) return CANCELLED;
        return yieldTask(context.signal).then(() => context.signal.aborted ? CANCELLED : OK, error => {
          if (context.signal.aborted) return CANCELLED;
          throw error;
        });
      };
      return status instanceof Promise ? status.then(complete) : complete(status);
    },
    channel_try_send(context, id, value, managed) {
      return commitSend(channels.get(id), payload(value, managed), context);
    },
    channel_receive_status(context, id) {
      const channel = channels.get(id);
      return channel ? awaitStatus(channel.receivers, context, () => claimReceive(channel, context)) : CLOSED;
    },
    channel_try_receive(context, id) { return claimReceive(channels.get(id), context); },
    channel_receive_value(context, id) { return consume(channels.get(id), context); },
    channel_close(_context, id) {
      const channel = channels.get(id);
      if (!channel || channel.closed) return;
      channel.closed = true;
      wakeAll(channel.senders);
      wakeAll(channel.receivers);
      channelChanged(channel);
    },
    hub_create() {
      if (nextHub > signedI64Max) return -1n;
      const id = nextHub++;
      hubs.set(id, { entries: [], cursor: 0, waiters: new Set() });
      return id;
    },
    hub_register(_context, id, index, channelId) {
      const hub = hubs.get(id);
      const channel = channels.get(channelId);
      return hub && channel ? changeRegistration(hub, index, channel) : HUB_NOT_FOUND;
    },
    hub_unregister(_context, id, index) {
      const hub = hubs.get(id);
      return hub ? changeRegistration(hub, index, undefined) : HUB_NOT_FOUND;
    },
    hub_wait_receive_status(context, id) {
      const hub = hubs.get(id);
      return hub ? awaitStatus(hub.waiters, context, () => pollHub(hub, context)) : HUB_NOT_FOUND;
    },
    hub_wait_receive_index(context, id) {
      const hub = hubs.get(id);
      const receipts = hubReceipts.get(context.owner);
      const entry = receipts?.get(hub);
      if (entry && !entry.channel) receipts.delete(hub);
      return entry?.index ?? -1n;
    },
    hub_wait_receive_value(context, id) {
      const hub = hubs.get(id);
      const receipts = hubReceipts.get(context.owner);
      const entry = receipts?.get(hub);
      check(entry?.channel, "Hub receipt belongs to another browser continuation");
      receipts.delete(hub);
      return consume(entry.channel, context);
    },
  };

  return {
    methods,
    context(owner, fiber, fiberSignal) {
      const signal = fiberSignal ? AbortSignal.any([shutdown.signal, fiberSignal]) : shutdown.signal;
      return { owner, fiber, signal };
    },
    release(owner) {
      for (const channel of claimed.get(owner) ?? []) {
        const node = { value: channel.receipt.value, next: channel.front };
        channel.front = node;
        channel.back ??= node;
        channel.count++;
        channel.receipt = null;
        channelChanged(channel);
      }
      claimed.delete(owner);
      hubReceipts.delete(owner);
    },
    dispose() {
      shutdown.abort(new DOMException("Beskid browser instance is disposed", "AbortError"));
      claimed.clear();
      hubReceipts.clear();
      channels.clear();
      hubs.clear();
      mutexes.clear();
      waitGroups.clear();
    },
  };
}
