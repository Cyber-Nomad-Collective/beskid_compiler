# Beskid Compiler

Beskid exists because I’m done with C#/.NET’s abstraction hell and runtime identity crisis.
This compiler is where language features are *language features* — not ten layers of corelib and DI containers.

The entire point of this project is to be more precise with hating microsoft for the lack of direction in csharp.

### IoC? Yes. IoC frameworks? No.

IoC is a good direction, but it belongs in the **compiler**, not in a forest of framework glue.

- No DI container black boxes.
- No “inject everything because we can.”
- If IoC exists, it should be **explicit, verifiable, and compiled**, not hidden behind runtime indirection.

### The pain points we’re cutting out

- **Corelib abstraction sprawl.** When core functionality becomes a maze of wrappers, you’re not writing software, you’re negotiating with an API.
- **Reflection as a crutch.** Slow, opaque, runtime‑only power instead of compile‑time truth.
- **CIL stagnation.** Language features outpace the VM, so everything devolves into workarounds and performance tax.

### What Beskid pushes forward

- **Enumerators and iteration as first‑class language features.**
- **Metaprogramming over reflection.** Compile‑time power, real guarantees.
- **Assembly output without IL handcuffs.**

### Non‑Goals

- Being “enterprise-friendly.”
- Competing with a colossal framework stack.
- Hiding complexity behind runtime containers.

### Status

Opinionated compiler project. Not finished. Not apologizing.
