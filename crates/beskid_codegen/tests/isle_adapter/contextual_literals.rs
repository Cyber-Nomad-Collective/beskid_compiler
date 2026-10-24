//! Contextual typing of numeric literals reaches lowering as ordinary typed literals: the owner's
//! networking example (`IpAddress::V4(127, 0, 0, 1)`, `PortBytes(0, 0)`,
//! `TcpListener.Bind(any, 8)`) is written without suffixes against the real Corelib `Network`
//! package, and every literal takes the width of its enum payload, parameter, or match context.
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
fn networking_example_without_suffixes_types_every_literal_from_its_context_and_lowers() {
    let (input, isa, root) =
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
    lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: loopback, symbol: "Loopback4".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
        ],
    )
    .expect("the networking example lowers with contextually typed literals");
}
