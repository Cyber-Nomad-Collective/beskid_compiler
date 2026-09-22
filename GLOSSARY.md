# Beskid glossary

## Executable host

The single portable C bootstrap linked into every Beskid native executable. It
owns ABI-v5 process initialization and shutdown, transfers process arguments to
`Core.Args` when that facade is used, then invokes the selected Beskid program
entry through the fixed native boundary.

## Core.IO transfer loop

The sole Corelib implementation responsible for descriptor transfer progress,
EOF, no-progress, and close semantics. The executable host must not duplicate
any of these policies; it only establishes process lifetime before Corelib code
runs.

## Network opaque resource

A `TcpStream`, `TcpListener`, or `UdpSocket` value that owns a private,
generation-tagged runtime handle. Its public Beskid shape exposes typed network
operations only; it never exposes a descriptor, platform constant, or native
status value.

## Canonical Corelib service source

The one compiler-embedded Corelib source unit authorized to call a private
ABI-v5 service family. For Networking this is `Network/Internal.bd`; public
Network modules call typed internal wrappers rather than importing raw services
themselves.
