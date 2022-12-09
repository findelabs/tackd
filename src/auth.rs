use axum::{
    http::header::AUTHORIZATION,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use headers::authorization::Credentials;

use crate::State;
use crate::users::{CurrentUser, Role};

pub async fn auth<B>(mut req: Request<B>, next: Next<B>) -> Result<Response, StatusCode> {
    // If there is no authorization header, return no user
    let auth_header = if let Some(auth_header) = req.headers().get(AUTHORIZATION) {
        log::debug!("\"Found authorization header\"");
        auth_header
    } else {
        log::debug!("\"Did not find authorization header\"");
        req.extensions_mut().insert(CurrentUser { id: None , role: Role { role: "upload".to_string()}});
        return Ok(next.run(req).await);
    };

    // Try and decode authorization header, returning api key and secret
    let (username, password) = match headers::authorization::Basic::decode(auth_header) {
        Some(o) => {
            log::debug!("\"Extracted basic creds from authorization header\"");
            (o.username().to_owned(), o.password().to_owned())
        }
        None => return Err(StatusCode::UNAUTHORIZED),
    };

    let state = req.extensions().get::<State>().expect("Missing State");

    // Search for api key and return user id
    match state
        .users_admin
        .validate_user_or_api_key(&username, &password)
        .await
    {
        Ok(current_user) => {
            log::debug!("\"Validated user as {:?}\"", &current_user);
            req.extensions_mut()
                .insert(current_user);
            return Ok(next.run(req).await);
        }
        Err(_) => {
            log::warn!("\"Unable to validate user or api key: {}\"", username);
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}
