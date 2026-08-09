use axum::{
    http::{HeaderMap, header},
    response::Response,
};
use beskid_pckg_auth::verify_pckg_session;
use beskid_pckg_community::{Principal, Role, Subject};

use super::{error::unauthorized, state::CommunityState};

// Axum responses intentionally carry the complete HTTP rejection payload here;
// boxing it would only add allocation and dereferencing at every route boundary.
#[allow(clippy::result_large_err)]
pub(super) fn authenticated_principal(state: &CommunityState, headers: &HeaderMap) -> Result<Principal, Response> {
    let Some(session_secret) = state.session_secret.as_deref() else {
        return Err(unauthorized());
    };
    let Some(session) = session_cookie(headers) else {
        return Err(unauthorized());
    };
    let identity = verify_pckg_session(session, session_secret).map_err(|_| unauthorized())?;
    let subject = Subject::new(identity.subject).map_err(|_| unauthorized())?;
    Ok(Principal::auth_hub(subject, [Role::User]))
}

fn session_cookie(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .map(str::trim)
        .find_map(|part| part.strip_prefix("pckg_session="))
}
