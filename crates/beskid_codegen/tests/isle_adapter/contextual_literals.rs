//! Contextual typing of numeric literals reaches lowering as ordinary typed literals: the owner's
//! networking example (`IpAddress::V4(127, 0, 0, 1)`, `PortBytes(0, 0)`,
//! `TcpListener.Bind(any, 8)`) is written without suffixes against the real Corelib `Network`
//! package, and every literal takes the width of its enum payload, parameter, or match context.
//! Lowering is proven on the same shapes declared locally (module emission of a single test
//! module cannot import the Corelib callees).
use super::support::{
    SyntaxGenerationId, SyntaxModuleItem, canonical_corelib_project_fixture, find_nodes_of_kind, lower_syntax_program,
    named_function,
};
use beskid_queries::{IndexedNodeKind, SemanticTypeId, contextual_integer_literal_abi_type};

const NETWORK_SOURCE: &str = r#"
use Core.Results;
use Network.Types;
use Network.Tcp.TcpListener;

SocketAddress Loopback4(i64 port) {
    Port value = match Network.Types.PortNumber(port) { Result::Ok(p) => p, Result::Error(_) => Network.Types.PortBytes(0, 0), };
    return SocketAddress { address: IpAddress::V4(127, 0, 0, 1), port: value };
}

i64 Main() {
    return match TcpListener.Bind(Loopback4(0), 8) { Result::Ok(_) => 0, Result::Error(_) => 1, };
}
"#;

#[test]
fn networking_example_without_suffixes_types_every_literal_from_its_context() {
    let (input, _isa, root) =
        canonical_corelib_project_fixture(&[("App/Main.bd", NETWORK_SOURCE)], SyntaxGenerationId(267));
    let db = input.database();
    let loopback = named_function(&input, root, "Loopback4");
    let main = named_function(&input, root, "Main");
    let widths = |item| {
        find_nodes_of_kind(db, item, IndexedNodeKind::Literal)
            .into_iter()
            .map(|literal| contextual_integer_literal_abi_type(db, literal).expect("contextual literal fact"))
            .collect::<Vec<_>>()
    };
    let u8 = Some(SemanticTypeId::U8);
    let i64 = Some(SemanticTypeId::I64);
    assert_eq!(widths(loopback), vec![u8, u8, u8, u8, u8, u8], "PortBytes(0, 0) and IpAddress::V4(127, 0, 0, 1)");
    assert_eq!(widths(main), vec![i64, i64, i64, i64], "Loopback4(0), Bind(.., 8) and the i64 match arms");
}

const LOCAL_NETWORK_SOURCE: &str = r#"
pub enum IpAddress { V4(u8 a, u8 b, u8 c, u8 d), V6(u32 a, u32 b, u32 c, u32 d), }
pub type Port { u8 high, u8 low, }
pub type SocketAddress { IpAddress address, Port port, }
pub Port PortBytes(u8 high, u8 low) { return Port { high: high, low: low }; }
pub i64 Bind(SocketAddress address, i64 backlog) { return backlog; }
SocketAddress Loopback4() { return SocketAddress { address: IpAddress::V4(127, 0, 0, 1), port: PortBytes(0, 80) }; }
i64 Main() { return Bind(Loopback4(), 8); }
"#;

/// The same shapes declared locally so the whole program lowers in one module: enum payloads,
/// type literal fields and parameters receive their literals at the declared widths.
#[test]
fn networking_shapes_without_suffixes_lower_with_contextual_widths() {
    let (input, isa, root) =
        canonical_corelib_project_fixture(&[("App/Main.bd", LOCAL_NETWORK_SOURCE)], SyntaxGenerationId(268));
    let items = ["PortBytes", "Bind", "Loopback4", "Main"]
        .iter()
        .map(|name| SyntaxModuleItem { key: named_function(&input, root, name), symbol: (*name).into() })
        .collect::<Vec<_>>();
    let db = input.database();
    let loopback = named_function(&input, root, "Loopback4");
    let widths = find_nodes_of_kind(db, loopback, IndexedNodeKind::Literal)
        .into_iter()
        .map(|literal| contextual_integer_literal_abi_type(db, literal).expect("contextual literal fact"))
        .collect::<Vec<_>>();
    assert_eq!(widths, vec![Some(SemanticTypeId::U8); 6], "IpAddress::V4(127, 0, 0, 1) and PortBytes(0, 80)");
    let main = named_function(&input, root, "Main");
    let widths = find_nodes_of_kind(db, main, IndexedNodeKind::Literal)
        .into_iter()
        .map(|literal| contextual_integer_literal_abi_type(db, literal).expect("contextual literal fact"))
        .collect::<Vec<_>>();
    assert_eq!(widths, vec![Some(SemanticTypeId::I64)], "Bind(.., 8)");
    lower_syntax_program(&input, isa.as_ref(), &items).expect("the networking shapes lower with contextually typed literals");
}
