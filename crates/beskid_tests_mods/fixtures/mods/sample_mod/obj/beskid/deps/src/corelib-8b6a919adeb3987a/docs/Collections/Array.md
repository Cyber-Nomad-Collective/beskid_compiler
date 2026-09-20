# Core.Collections.Array

`Core.Collections.Array` operates on managed typed `T[]` values. `Empty<T>` allocates from the descriptor for `T`; no fixed element stride is supplied by collection source.

`Len`, `Get`, and `Set` use logical-length checked typed array semantics. `Capacity` reports allocation capacity without exposing uninitialized slots. `Append` is the sole length-extending operation; compiler lowering owns overflow validation and the rooted grow, typed store/barrier, owner publication, and exactly-once finish protocol.

`Iterate<T>` captures the logical length. `HasNext` stops at that length, and `Current` reads the current element through the same direct bounds semantics.

Public helpers: `Empty`, `Len`, `Capacity`, `IsEmpty`, `Get`, `Set`, `Append`, `Clear`, `RemoveLast`, `CopyRange`, `Iterate`, `HasNext`, `Current`, `Advance`, and `Index`.
