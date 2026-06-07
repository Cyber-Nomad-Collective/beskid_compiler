//! Rewrite declaration-site identifiers toward case profiles before pretty-printing.

use crate::naming_case::{is_keyword_escape, matches_profile, normalize_to_profile};
use crate::naming_program::{NamingRole, walk_program_mut};
use crate::syntax::{Identifier, Program};

/// Normalize identifier spellings in `program` toward platform-spec case profiles.
pub fn normalize_program_naming(program: &mut Program) {
    walk_program_mut(program, |role, ident| {
        normalize_role_ident(role, ident);
    });
}

fn normalize_role_ident(role: NamingRole, ident: &mut Identifier) {
    if ident.name == "self" || is_keyword_escape(&ident.name) {
        return;
    }
    let profile = role.profile();
    if matches_profile(&ident.name, profile) {
        return;
    }
    ident.name = normalize_to_profile(&ident.name, profile);
}
