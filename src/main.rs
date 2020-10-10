#![allow(dead_code)]

#[macro_use(lazy_static)]
extern crate lazy_static;

use mimalloc::MiMalloc;
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

mod admin_functions;
mod auth;
mod comments;
mod orchestrator;
mod posts;
mod ratelimiter;
mod responses;
mod utils;
mod writs;

use actix_files::NamedFile;
use actix_web::{get, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::{fs::File, io::BufReader, sync::Arc};
use tera::{Context, Tera};
use time::Duration;

use rustls::internal::pemfile::{certs, pkcs8_private_keys};
use rustls::{NoClientAuth, ServerConfig};

use orchestrator::Orchestrator;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let matches = clap::App::new("Grimstack")
        .version("0.0.1")
        .author("Saul <me@saul.app>")
        .about("multipurpose web app database thingy")
        .arg(
            clap::Arg::with_name("production")
                .short("p")
                .long("production")
                .help("run the server in production mode"),
        )
        .arg(
            clap::Arg::with_name("domain")
                .short("d")
                .long("domain")
                .help("set the server's domain name"),
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

    admin_functions::watch_and_update_files();
    println!("file watching active");
    // if CONF.read().dev_mode {}

    HttpServer::new(|| {
        App::new().service(web::resource("*").route(web::get().to(|req: HttpRequest| {
            HttpResponse::Found()
                .header(
                    actix_web::http::header::LOCATION,
                    format!("https://{}{}", CONF.read().domain, req.path()),
                )
                .finish()
                .into_body()
        })))
    })
    .disable_signals()
    .bind("0.0.0.0:80")
    // .bind("0.0.0.0:8080")
    .unwrap()
    .run();

    let cert_file =
        &mut BufReader::new(File::open(CONF.read().cert_path.clone().as_str()).unwrap());
    let key_file =
        &mut BufReader::new(File::open(CONF.read().privkey_path.clone().as_str()).unwrap());

    let mut tls_config = ServerConfig::new(NoClientAuth::new());
    let cert_chain = certs(cert_file).unwrap();
    let mut keys = pkcs8_private_keys(key_file).unwrap();

    if let Err(e) = tls_config.set_single_cert(cert_chain, keys.remove(0)) {
        println!("tls_config.set_single_cert failed: {}", e);
        std::process::exit(1);
    }

    let orc = Arc::new(Orchestrator::new(60 * 60 * 24 * 7 * 2));

    let orc_clone = orc.clone();

    let app_server = HttpServer::new(move || {
        App::new()
            .wrap(actix_web::middleware::Compress::default())
            .data(orc_clone.clone())
            .service(index)
            .service(auth::check_authentication)
            .service(auth::auth_attempt)
            .service(auth::logout)
            .service(auth::administer_administrality)
            .service(auth::remove_administrality)
            .service(auth::check_administrality)
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
            .service(comments::make_comment)
            .service(comments::delete_comment)
            .service(comments::upvote_comment)
            .service(comments::unvote_comment)
            .service(comments::downvote_comment)
            .service(posts::render_post)
            .service(posts::render_post_by_slug)
            .service(admin_functions::remote_http)
            .service(admin_panel)
            .service(admin_gateway)
            .service(serve_files_and_templates)
    })
    // .bind_rustls("0.0.0.0:8443", tls_config)?
    .bind_rustls("0.0.0.0:443", tls_config)?
    .backlog(4096)
    .run()
    .await;

    app_server
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Config {
    pub domain: String,
    pub db_location: String,
    pub admin_key: String,
    pub dev_mode: bool,
    do_token: String,
    cert_path: String,
    privkey_path: String,
}

lazy_static! {
    pub static ref TEMPLATES: RwLock<Tera> = {
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
    };
    pub static ref CONF: RwLock<Config> = {
        let config_toml = std::fs::read_to_string("./private/config.toml")
            .expect("couldn't read the config file");
        let config: Config = toml::from_str(&config_toml).expect("config file is broken TOML");
        RwLock::new(config)
    };
}

#[get("/")]
async fn index(req: HttpRequest, orc: web::Data<Arc<Orchestrator>>) -> impl Responder {
    let mut ctx = Context::new();
    let (o_usr, potential_renewal_cookie) = orc.user_by_session_renew(&req, Duration::days(3));
    if let Some(usr) = o_usr {
        ctx.insert("username", &usr.username);
        ctx.insert("dev_mode", &orc.dev_mode);
        ctx.insert(
            "is_writer",
            &orc.user_has_some_attrs(&usr.id, &["writer", "admin"])
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
            if orc.dev_mode {
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
async fn admin_panel(req: HttpRequest, orc: web::Data<Arc<Orchestrator>>) -> HttpResponse {
    let mut ctx = Context::new();
    if let Some(usr) = orc.admin_by_session(&req) {
        ctx.insert("username", &usr.username);
        ctx.insert("dev_mode", &orc.dev_mode);
    } else {
        return HttpResponse::Found()
            .header(actix_web::http::header::LOCATION, "/admin-gateway")
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

#[get("/admin-gateway")]
async fn admin_gateway(req: HttpRequest, orc: web::Data<Arc<Orchestrator>>) -> HttpResponse {
    let mut ctx = Context::new();
    if let Some(usr) = orc.user_by_session(&req) {
        ctx.insert("username", &usr.username);
        ctx.insert("dev_mode", &orc.dev_mode);
    } else {
        return HttpResponse::Found()
            .header(actix_web::http::header::LOCATION, "/")
            .finish()
            .into_body();
    }
    if let Ok(s) = TEMPLATES.read().render("admin-gateway.html", &ctx) {
        return HttpResponse::Ok().content_type("text/html").body(s);
    }
    HttpResponse::InternalServerError()
        .content_type("text/plain")
        .body("The home page template is broken! :( We have failed you.")
}

#[get("/*")]
async fn serve_files_and_templates(
    req: HttpRequest,
    orc: web::Data<Arc<Orchestrator>>,
) -> HttpResponse {
    let path: &str = req.path();
    if path.ends_with("/") {
        return HttpResponse::Found()
            .header(
                actix_web::http::header::LOCATION,
                path.trim_end_matches("/"),
            )
            .finish()
            .into_body();
    }

    if path.contains("admin") {
        if let Some(usr) = orc.admin_by_session(&req) {
            let asset_dir = format!("./assets{}", path);
            if let Ok(file) = NamedFile::open(&asset_dir) {
                if let Ok(file_response) = file.into_response(&req) {
                    return file_response;
                }
                return HttpResponse::Unauthorized()
                    .content_type("text/plain")
                    .body("Non-admins may not load admin only content");
            }

            let mut name = path.trim_start_matches('/').to_string();

            let is_js = name.contains(".js");
            if !is_js && !name.contains(".html") {
                name = name + ".html";
            }

            let mut ctx = Context::new();
            ctx.insert("username", &usr.username);
            ctx.insert("dev_mode", &orc.dev_mode);

            if is_js && name.contains("admin-do-ddns") {
                let do_token: String = CONF.read().do_token.clone();
                ctx.insert("do_token", &do_token);
            }

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
    } else {
        let asset_dir = format!("./assets{}", req.path());
        if let Ok(file) = NamedFile::open(&asset_dir) {
            if let Ok(file_response) = file.into_response(&req) {
                return file_response;
            }
        }

        let mut name = path.trim_start_matches('/').to_string();
        let is_js = name.contains(".js");
        if !is_js && !name.contains(".html") {
            name = name + ".html";
        }

        let mut ctx = Context::new();
        ctx.insert("dev_mode", &orc.dev_mode);

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

    HttpResponse::NoContent()
        .content_type("text/html")
        .body("Take heed, this page is either broken, forbidden, or nonexistent :(")
}
