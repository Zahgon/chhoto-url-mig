// SPDX-FileCopyrightText: 2023-2026 Sayantan Santra <sayantan.santra689@gmail.com>
// SPDX-License-Identifier: MIT

use rocket::http::{ContentType, Status};
use rocket::response::Redirect;
use rocket::response::content::RawJson;
use rocket::{State, get};

use crate::{
    AppState,
    auth::Auth,
    database,
    services::types::{
        BackendConfig,
        ChhotoError::{ClientError, ServerError},
        GetReqParams,
    },
    utils,
};

// Return all active links
#[get("/api/all?<page_after>&<page_no>&<page_size>&<filter>")]
pub(crate) async fn getall(
    auth: Auth,
    data: &State<AppState>,
    page_after: Option<String>,
    page_no: Option<i64>,
    page_size: Option<i64>,
    filter: Option<String>,
) -> (Status, (ContentType, String)) {
    let params = GetReqParams {
        page_after,
        page_no,
        page_size,
        filter,
    };
    match auth {
        Auth::None { result: _ } => (
            Status::Unauthorized,
            (ContentType::Plain, "Unauthorized".to_owned()),
        ),
        Auth::InvalidAPIKey { result } => {
            (Status::Unauthorized, (ContentType::Plain, result.reason))
        }
        _ => {
            let reader = data.reader.lock().await;
            match utils::getall_helper(&reader, params) {
                Ok(s) => (Status::Ok, (ContentType::JSON, s)),
                Err(ServerError) => (
                    Status::InternalServerError,
                    (
                        ContentType::Plain,
                        "Something went wrong while loading the links.".to_owned(),
                    ),
                ),
                Err(ClientError { reason }) => {
                    (Status::BadRequest, (ContentType::Plain, reason))
                }
            }
        }
    }
}

// Get the site URL
// This is deprecated, and might be removed in the future.
// Use /api/getconfig instead
#[get("/api/siteurl")]
pub(crate) async fn siteurl(data: &State<AppState>) -> (Status, (ContentType, String)) {
    if let Some(url) = &data.config.site_url {
        (Status::Ok, (ContentType::Plain, url.clone()))
    } else {
        (Status::Ok, (ContentType::Plain, "unset".to_owned()))
    }
}

// Get the version number
// This is deprecated, and might be removed in the future.
// Use /api/getconfig instead
#[get("/api/version")]
pub(crate) async fn version() -> (Status, (ContentType, String)) {
    (
        Status::Ok,
        (
            ContentType::Plain,
            format!("Chhoto URL v{}", utils::get_version()),
        ),
    )
}

// Get the user's current role
#[get("/api/whoami")]
pub(crate) async fn whoami(data: &State<AppState>, auth: Auth) -> (Status, (ContentType, String)) {
    let config = &data.config;
    let acting_user = match auth {
        Auth::ValidAPIKey | Auth::ValidSession => "admin",
        _ => {
            if config.public_mode {
                "public"
            } else {
                "nobody"
            }
        }
    };
    (
        Status::Ok,
        (ContentType::Plain, acting_user.to_owned()),
    )
}

// Get some useful backend config
#[get("/api/getconfig")]
pub(crate) async fn getconfig(auth: Auth, data: &State<AppState>) -> (Status, RawJson<String>) {
    let config = &data.config;
    let ok_response = || {
        let backend_config = BackendConfig {
            version: utils::get_version(),
            allow_capital_letters: config.allow_capital_letters,
            public_mode: config.public_mode,
            public_mode_expiry_delay: config.public_mode_expiry_delay.unwrap_or_default(),
            allowed_protocols: config.allowed_protocols.clone(),
            site_url: config.site_url.clone(),
            slug_style: config.slug_style.to_string(),
            slug_length: config.slug_length,
            try_longer_slug: config.try_longer_slug,
            frontend_page_size: config.frontend_page_size,
        };
        (
            Status::Ok,
            RawJson(serde_json::to_string(&backend_config).unwrap_or_default()),
        )
    };
    match auth {
        Auth::ValidSession | Auth::ValidAPIKey => ok_response(),
        Auth::None { result } | Auth::InvalidAPIKey { result } => {
            if data.config.public_mode {
                ok_response()
            } else {
                (
                    Status::Unauthorized,
                    RawJson(serde_json::to_string(&result).unwrap_or_default()),
                )
            }
        }
    }
}

// Handle a given shortlink
#[get("/<shortlink>", rank = 10)]
pub(crate) async fn link_handler(
    shortlink: &str,
    data: &State<AppState>,
) -> Result<Redirect, Status> {
    let longlink = {
        let reader = data.reader.lock().await;
        database::find_and_add_hit(shortlink, &reader, &data.hits_tx)
    };
    if let Ok(longlink) = longlink {
        if data.config.use_temp_redirect {
            Ok(Redirect::to(longlink))
        } else {
            // Defaults to permanent redirection
            Ok(Redirect::permanent(longlink))
        }
    } else {
        // Return the status so the registered 404 catcher renders the page deterministically.
        Err(Status::NotFound)
    }
}
