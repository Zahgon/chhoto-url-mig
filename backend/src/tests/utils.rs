// SPDX-FileCopyrightText: 2023-2026 Sayantan Santra <sayantan.santra689@gmail.com>
// SPDX-License-Identifier: MIT

use std::{fmt::Display, rc::Rc};

use rocket::http::{Header, Status};
use rocket::local::asynchronous::Client;
use serde::Deserialize;
use tempfile::TempDir;

use crate::*;

#[derive(Deserialize)]
pub(super) struct URLData {
    #[serde(default, alias = "shorturl")]
    pub(super) shortlink: String,
    #[serde(default, alias = "longurl")]
    pub(super) longlink: String,
    #[serde(default)]
    pub(super) hits: i64,
    #[serde(default)]
    pub(super) expiry_time: i64,
    #[serde(default)]
    pub(super) notes: String,
    #[serde(default)]
    pub(super) reason: String,
}

#[derive(Deserialize)]
pub(super) struct BackendConfig {
    pub(super) version: String,
    pub(super) slug_length: usize,
}

pub(super) fn default_config(test: &str) -> config::Config {
    config::Config {
        listen_address: String::from("0.0.0.0"),
        port: 4567,
        db_location: format!("/tmp/chhoto-url-test-{test}.sqlite"),
        cache_control_header: None,
        disable_frontend: true,
        site_url: Some(String::from("https://mydomain.com")),
        public_mode: false,
        public_mode_expiry_delay: None,
        use_temp_redirect: false,
        allowed_protocols: Vec::from(["http", "https", "ftp", "magnet"].map(|s| s.to_string())),
        password: Some(String::from("testpass")),
        hash_algorithm: config::HashAlgorithm::None,
        api_key: Some(String::from(
            "Z8FNjh2J2v3yfb0xPDIVA58Pj4D0e2jSERVdoqM5pJCbU2w5tmg3PNioD6GUhaQwHHaDLBNZj0EQE8MS4TLKcUyusa05",
        )),
        slug_style: config::SlugStyle::Pair,
        slug_length: 8,
        try_longer_slug: false,
        allow_capital_letters: false,
        custom_landing_directory: None,
        use_wal_mode: true,
        ensure_acid: false,
        frontend_page_size: 10,
    }
}

pub(super) async fn create_app(conf: &config::Config, test: &str) -> (TempDir, Client) {
    let tempdir = TempDir::new().unwrap();
    let db_file = tempdir.path().join(format!("{test}.sqlite"));
    let writer = Arc::from(Mutex::from(database::open_db(
        db_file.to_str().unwrap(),
        false,
    )));
    database::init_db(
        &mut *writer.lock().await,
        conf.use_wal_mode,
        conf.ensure_acid,
    );

    let (hits_tx, hits_rx) = mpsc::channel::<(String, bool)>(1024);
    background::spawn_hits_worker(Arc::clone(&writer), hits_rx);

    let app_state = AppState {
        hits_tx,
        reader: Mutex::new(database::open_db(db_file.to_str().unwrap(), false)),
        writer,
        config: conf.clone(),
    };

    let secret_key: [u8; 64] = rand::random();
    let figment = rocket::Config::figment()
        .merge(("secret_key", secret_key.as_slice()))
        .merge(("cli_colors", false))
        .merge(("log_level", rocket::config::LogLevel::Off))
        .merge(("ident", false));

    let rocket_instance = rocket::custom(figment)
        .manage(app_state)
        .mount(
            "/",
            rocket::routes![
                services::link_handler,
                services::edit_link,
                services::getall,
                services::siteurl,
                services::version,
                services::getconfig,
                services::add_links,
                services::delete_link,
                services::login,
                services::logout,
                services::expand,
                services::whoami
            ],
        )
        .register("/", rocket::catchers![services::utils::error404]);

    let client = Client::tracked(rocket_instance).await.unwrap();

    (tempdir, client)
}

pub(super) async fn add_link<S: Display>(
    app: &Client,
    api_key: &str,
    shortlink: S,
    expiry_delay: i64,
    notes: &str,
) -> (Status, URLData) {
    let resp = app
        .post("/api/new")
        .header(Header::new("X-API-Key", api_key.to_owned()))
        .body(format!(
            "{{\"shortlink\":\"{shortlink}\",\"longlink\":\"https://example-{shortlink}.com\",\"expiry_delay\":{expiry_delay},\"notes\":\"{notes}\"}}"
        ))
        .dispatch()
        .await;

    let status = resp.status();
    let body = resp.into_string().await.unwrap();
    let url: URLData = serde_json::from_str(&body).unwrap();

    (status, url)
}

pub(super) async fn expand<S: Display>(
    app: &Client,
    api_key: &str,
    shortlink: S,
) -> (Status, URLData) {
    let resp = app
        .post("/api/expand")
        .header(Header::new("X-API-Key", api_key.to_owned()))
        .body(shortlink.to_string())
        .dispatch()
        .await;

    let status = resp.status();
    let body = resp.into_string().await.unwrap();
    let url: URLData = serde_json::from_str(&body).unwrap();

    (status, url)
}

pub(super) async fn getall(app: &Client, api_key: &str, params: &str) -> Rc<[URLData]> {
    let resp = app
        .get(format!("/api/all?{params}"))
        .header(Header::new("X-API-Key", api_key.to_owned()))
        .dispatch()
        .await;

    assert!(resp.status().class().is_success());
    let body = resp.into_string().await.unwrap();
    let reply_chunks: Rc<[URLData]> = serde_json::from_str(&body).unwrap();

    reply_chunks
}

pub(super) async fn edit_link(
    app: &Client,
    api_key: &str,
    shortlink: &str,
    reset_hits: bool,
    expiry_time: Option<i64>,
    notes: Option<&str>,
) -> Status {
    let mut payload = format!(
        "\"shortlink\":\"{shortlink}\",\"longlink\":\"https://edited-{shortlink}.com\",\"reset_hits\":{reset_hits}"
    );
    if let Some(expiry) = expiry_time {
        payload.push_str(&format!(",\"expiry_time\":{expiry}"));
    }
    if let Some(note) = notes {
        payload.push_str(&format!(",\"notes\":\"{note}\""));
    }
    let resp = app
        .put("/api/edit")
        .header(Header::new("X-API-Key", api_key.to_owned()))
        .body(format!("{{{payload}}}"))
        .dispatch()
        .await;
    resp.status()
}
