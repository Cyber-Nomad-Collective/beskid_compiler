`Testing.Contracts` defines two **`contract`** protocols used when integrations need lazy evaluation (for example building expensive messages only on failure).

## Contracts

```beskid
pub contract AssertionPredicate {
    bool Check();
}

pub contract AssertionMessageBuilder {
    string Build();
}
```

Concrete implementations live in user or harness code; corelib only declares the protocols for documentation and type checking.
