use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "beskid_doc.pest"]
pub struct DocSyntaxParser;
