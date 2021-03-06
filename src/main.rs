// #![allow(dead_code)]
#![feature(once_cell)]
#![feature(iter_advance_by)]
#![feature(drain_filter)]

use mimalloc::MiMalloc;
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

mod admin_functions;
mod auth;
mod email;
mod expirable_data;
mod comments;
mod orchestrator;
mod posts;
mod ratelimiter;
mod responses;
mod utils;
mod writs;
mod websockets;

use actix_files::NamedFile;
use actix_web::{get, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::{fs::File, io::BufReader, lazy::SyncLazy};
use tera::{Context, Tera};
use time::Duration;

use rustls::internal::pemfile::{certs, pkcs8_private_keys};
use rustls::{NoClientAuth, ServerConfig};

use orchestrator::ORC;

pub static TEMPLATES: SyncLazy<RwLock<Tera>> = SyncLazy::new(|| {
    let mut tera = match Tera::new(&format!(
        "{}/templates/**/*",
        std::env::current_dir().unwrap().to_str().unwrap()
    )) {
        Ok(t) => t,
        Err(e) => {
            println!("Tera ran into trouble parsing your templates. errs: {}", e);
            std::process::exit(1);
        }
    };
    tera.autoescape_on(vec![]);
    RwLock::new(tera)
});

pub static CONF: SyncLazy<RwLock<Config>> = SyncLazy::new(|| {
    let config_toml =
        std::fs::read_to_string("./private/config.toml").expect("couldn't read the config file");
    let config: Config = toml::from_str(&config_toml).expect("config file is broken TOML");
    RwLock::new(config)
});

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let matches = clap::App::new("kurshok.space")
        .version("0.1.1")
        .author("Saul <saul@kurshok.space>")
        .about("multipurpose web app database thingy")
        .arg(
            clap::Arg::new("production")
                .short('p')
                .long("production")
                .about("run the server in production mode")
        )
        .arg(
            clap::Arg::new("domain")
                .short('d')
                .long("domain")
                .about("set the server's domain name")
        )
        .get_matches();

    let dm = !matches.is_present("production");
    if !dm && CONF.read().dev_mode {
        CONF.write().dev_mode = dm;
    }

    if let Some(domain) = matches.value_of("domain") {
        if domain != CONF.read().domain {
            CONF.write().domain = domain.to_string();
        }
    }

    if admin_functions::watch_and_update_files() {
        println!("file watching active");
    }

    expirable_data::start_system();
    println!("expirable_data system active");

    HttpServer::new(|| {
        App::new().service(
            web::resource("*").route(
                web::get().to(|req: HttpRequest| {
                    let path = req.path();
                    if path.contains(".well-known/acme-challenge") {
                        let challenge_dir = format!("./private/acme/{}", path);
                        if let Ok(file) = NamedFile::open(&challenge_dir) {
                            return file.into_response(&req);
                        }
                    }

                    HttpResponse::Found()
                        .append_header((
                            actix_web::http::header::LOCATION,
                            format!("https://{}{}", CONF.read().domain, path),
                        ))
                        .finish()
                        .into_body()
                })
            )
        )
    })
    .disable_signals()
    .bind("0.0.0.0:80").expect("failed to start :80 -> tls redirect server")
    .run();

    let app_server = start_server();

    // TODO: automatic letsencrypt tls cert renewal

    app_server.await
}

fn start_server() -> actix_web::dev::Server {
    let cert_file = &mut BufReader::new(File::open(CONF.read().cert_path.clone().as_str()).unwrap());
    let key_file = &mut BufReader::new(File::open(CONF.read().privkey_path.clone().as_str()).unwrap());

    let mut tls_config = ServerConfig::new(NoClientAuth::new());
    let cert_chain = certs(cert_file).unwrap();
    let mut keys = pkcs8_private_keys(key_file).unwrap();

    if let Err(e) = tls_config.set_single_cert(cert_chain, keys.remove(0)) {
        println!("tls_config.set_single_cert failed: {}", e);
        std::process::exit(1);
    }

    let app_server = HttpServer::new(|| {
        App::new()
            .wrap(actix_web::middleware::Compress::default())
            .service(index)
            .service(auth::check_authentication)
            .service(auth::auth_attempt)
            .service(auth::indirect_auth_verification)
            .service(auth::auth_email_status_check)
            .service(auth::auth_link)
            .service(auth::logout)
            .service(auth::change_user_detail)
            .service(auth::get_user_description)
            .service(writs::editable_writ_query)
            .service(writs::writ_query)
            .service(writs::push_raw_writ)
            .service(writs::delete_writ)
            .service(writs::upvote_writ)
            .service(writs::unvote_writ)
            .service(writs::downvote_writ)
            .service(writs::post_content)
            .service(writs::writ_raw_content)
            .service(comments::post_comment_query)
            .service(comments::edit_comment_request)
            .service(comments::fetch_comment_raw_content)
            .service(comments::make_comment)
            .service(comments::delete_comment)
            .service(comments::upvote_comment)
            .service(comments::unvote_comment)
            .service(comments::downvote_comment)
            .service(posts::render_post)
            .service(posts::render_post_by_slug)
            .service(web::resource("/ws").to(websockets::ws_conn_setup))
            .service(admin_functions::remote_http)
            .service(admin_functions::reload_templates_request)
            .service(admin_functions::expire_data_request)
            .service(admin_panel)
            .service(serve_files_and_templates)
    })
    .bind_rustls("0.0.0.0:443", tls_config).expect("actix_web server_startup failed to bind rustls")
    .backlog(4096)
    .run();

    app_server
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Config {
    pub domain: String,
    pub db_location: String,
    pub dev_mode: bool,
    pub admin_emails: Vec<String>,
    pub mail_server: String,
    pub smtp_username: String,
    pub smtp_password: String,
    cert_path: String,
    privkey_path: String,
}

#[get("/")]
async fn index(req: HttpRequest) -> impl Responder {
    let mut ctx = Context::new();
    let (o_usr, potential_renewal_cookie) = ORC.user_by_session_renew(&req, Duration::days(3));
    if let Some(usr) = o_usr {
        ctx.insert("username", &usr.username);
        ctx.insert("usr_id", &usr.id);
        ctx.insert("handle", &usr.handle);
        ctx.insert("dev_mode", &ORC.dev_mode);
        ctx.insert(
            "is_writer",
            &ORC.user_has_some_attrs(usr.id, &["writer", "admin"])
                .unwrap_or(false),
        );
    }

    match TEMPLATES.read().render("index.html", &ctx) {
        Ok(s) => match potential_renewal_cookie {
            Some(cookie) => HttpResponse::Ok()
                .cookie(cookie)
                .content_type("text/html")
                .body(s),
            None => HttpResponse::Ok().content_type("text/html").body(s),
        },
        Err(err) => {
            if ORC.dev_mode {
                HttpResponse::InternalServerError()
                    .content_type("text/plain")
                    .body(&format!("index.html template is broken - error : {}", err))
            } else {
                HttpResponse::InternalServerError()
                    .content_type("text/plain")
                    .body("The home page template is broken! :( We have failed you.")
            }
        }
    }
}

#[get("/admin")]
async fn admin_panel(req: HttpRequest) -> HttpResponse {
    let mut ctx = Context::new();
    if let Some(usr) = ORC.admin_by_session(&req) {
        ctx.insert("username", &usr.username);
        ctx.insert("dev_mode", &ORC.dev_mode);
    } else {
        return HttpResponse::Found()
            .append_header((actix_web::http::header::LOCATION, "/admin-gateway"))
            .finish()
            .into_body();
    }

    if let Ok(s) = TEMPLATES.read().render("admin.html", &ctx) {
        return HttpResponse::Ok().content_type("text/html").body(s);
    }
    HttpResponse::Ok()
        .content_type("text/plain")
        .body("The home page template is broken! :( We have failed you.")
}

#[get("/*")]
async fn serve_files_and_templates(req: HttpRequest) -> HttpResponse {
    let path: &str = req.path();
    if path.ends_with("/") {
        return HttpResponse::Found()
            .append_header((
                actix_web::http::header::LOCATION,
                path.trim_end_matches("/"),
            ))
            .finish()
            .into_body();
    }

    if path.contains("admin") {
        if let Some(usr) = ORC.admin_by_session(&req) {
            let asset_dir = format!("./assets{}", path);
            if let Ok(file) = NamedFile::open(&asset_dir) {
                return file.into_response(&req);
            }

            let mut name = path.trim_start_matches('/').to_string();

            let is_js = name.contains(".js");
            if !is_js && !name.contains(".html") {
                name = name + ".html";
            }

            let mut ctx = Context::new();
            ctx.insert("username", &usr.username);
            ctx.insert("dev_mode", &ORC.dev_mode);

            if let Ok(s) = TEMPLATES.read().render(&name, &ctx) {
                return HttpResponse::Ok()
                    .content_type(if is_js {
                        "application/javascript"
                    } else {
                        "text/html"
                    })
                    .body(s);
            }
        }

        return HttpResponse::Unauthorized()
                    .content_type("text/plain")
                    .body("Non-admins may not load admin only content");
    } else {
        let asset_dir = format!("./assets{}", req.path());
        if let Ok(file) = NamedFile::open(&asset_dir) {
            return file.into_response(&req);
        }

        let mut name = path.trim_start_matches('/').to_string();
        let is_js = name.contains(".js");
        if !is_js && !name.contains(".html") {
            name = name + ".html";
        }

        let mut ctx = Context::new();
        ctx.insert("dev_mode", &ORC.dev_mode);

        if let Ok(s) = TEMPLATES.read().render(&name, &ctx) {
            if s.len() == 0 {
                return HttpResponse::NoContent()
                    .content_type("text/html")
                    .body("Take heed, this page is either broken, forbidden, or nonexistent :(");
            }
            return HttpResponse::Ok().content_type(if is_js {
                "application/javascript"
            } else {
                "text/html"
            })
            .body(s);
        }
    }

    HttpResponse::NoContent()
        .content_type("text/html")
        .body("Take heed, this page is either broken, forbidden, or nonexistent :(")
}
