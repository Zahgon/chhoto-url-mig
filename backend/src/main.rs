// SPDX-FileCopyrightText: 2023-2026 Sayantan Santra <sayantan.santra689@gmail.com>
// SPDX-License-Identifier: MIT

use log::info;
use rocket::fairing::{Fairing, Info, Kind};
use rocket::fs::FileServer;
use rocket::http::Header;
use rocket::response::Redirect;
use rocket::{Request, Response, routes};
use rusqlite::Connection;
use std::sync::{Arc, Once};
use tokio::sync::{Mutex, mpsc};

// Import modules
mod auth;
mod background;
mod config;
mod database;
mod services;

use services::utils;

// Tests
#[cfg(test)]
mod tests;

// This struct represents state
struct AppState {
    hits_tx: mpsc::Sender<(String, bool)>,
    reader: Mutex<Connection>,
    writer: Arc<Mutex<Connection>>,
    config: config::Config,
}

static LOGGER: Once = Once::new();

// Fairing that adds a configurable Cache-Control header to every response.
struct CacheControlFairing {
    header: Option<String>,
}

#[rocket::async_trait]
impl Fairing for CacheControlFairing {
    fn info(&self) -> Info {
        Info {
            name: "Cache-Control Header",
            kind: Kind::Response,
        }
    }

    async fn on_response<'r>(&self, _req: &'r Request<'_>, res: &mut Response<'r>) {
        if let Some(header) = &self.header {
            res.set_header(Header::new("Cache-Control", header.to_owned()));
        }
    }
}

// Build the env_logger instance with the same custom format as before.
fn init_logger() {
    env_logger::builder()
        .parse_filters(
            std::env::var("RUST_LOG")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or("warn,chhoto_url=info".to_owned())
                .as_str(),
        )
        .format(|buf, record| {
            use chrono::Local;
            use env_logger::fmt::style::{AnsiColor, Style};
            use std::io::Write;

            let subtle = Style::new().fg_color(Some(AnsiColor::BrightBlack.into()));
            let level_style = buf.default_level_style(record.level());

            writeln!(
                buf,
                "{subtle}[{subtle:#}{} {level_style}{:<6}{level_style:#}{}{subtle}]{subtle:#} {}",
                Local::now().format("%Y-%m-%d %H:%M:%S%Z"),
                record.level(),
                record.module_path().unwrap_or_default(),
                record.args()
            )
        })
        .init();
}

// Redirect helper for the custom landing directory setup.
#[rocket::get("/admin/manage")]
fn admin_manage_redirect() -> Redirect {
    Redirect::to("/admin/manage/")
}

#[rocket::main]
async fn main() -> Result<(), rocket::Error> {
    init_logger();

    eprintln!("----------------------------------------------------------------------");
    info!("Starting Chhoto URL Server v{}", utils::get_version());
    info!("Source: https://github.com/SinTan1729/chhoto-url");
    eprintln!("----------------------------------------------------------------------");

    // Read config from env vars
    let conf = config::read();
    // ArcMutex is necessary since the writer is shared across threads
    let writer = Arc::new(Mutex::new(database::open_db(&conf.db_location, false)));

    // Initialize the database and perform migrations
    let use_wal_mode = conf.use_wal_mode;
    database::init_db(&mut *writer.lock().await, use_wal_mode, conf.ensure_acid);
    // Spawn cleaner
    background::spawn_cleaner(Arc::clone(&writer), use_wal_mode);
    // Spawn hit updater
    let (hits_tx, hits_rx) = mpsc::channel::<(String, bool)>(1024);
    background::spawn_hits_worker(Arc::clone(&writer), hits_rx);

    let port = conf.port;
    let addr = conf.listen_address.clone();

    // Generate session key in runtime so that restart invalidates older logins
    let secret_key: [u8; 64] = rand::random();

    // Configure Rocket via figment: bind address/port and set the secret key
    // used to sign the private session cookie.
    let figment = rocket::Config::figment()
        .merge(("address", addr.clone()))
        .merge(("port", port))
        .merge(("secret_key", secret_key.as_slice()))
        .merge(("cli_colors", false))
        .merge(("log_level", rocket::config::LogLevel::Off))
        .merge(("ident", false));

    let app_state = AppState {
        hits_tx,
        reader: Mutex::new(database::open_db(&conf.db_location, true)),
        writer: Arc::clone(&writer),
        config: conf.clone(),
    };

    let disable_frontend = conf.disable_frontend;
    let custom_landing_directory = conf.custom_landing_directory.clone();
    let cache_control_header = conf.cache_control_header.clone();

    let mut rocket_instance = rocket::custom(figment)
        .manage(app_state)
        .attach(CacheControlFairing {
            header: cache_control_header,
        })
        .mount(
            "/",
            routes![
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
                services::whoami,
            ],
        )
        .register("/", rocket::catchers![services::utils::error404]);

    if !disable_frontend {
        if let Some(dir) = &custom_landing_directory {
            rocket_instance = rocket_instance
                .mount("/", routes![admin_manage_redirect])
                .mount("/admin/manage", FileServer::from("./frontend/"))
                .mount("/", FileServer::from(dir));
        } else {
            rocket_instance = rocket_instance.mount("/", FileServer::from("./frontend/"));
        }
    }

    LOGGER.call_once(|| {
        info!(
            "Server has started listening to {} on port {}.",
            &addr, port
        );
    });

    rocket_instance.launch().await?;
    Ok(())
}
