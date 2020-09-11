use std::sync::Arc;

use actix_web::{get, web, HttpRequest, HttpResponse};
use chrono::Duration;
use tera::Context;

use super::TEMPLATES;

use crate::writs::WritQuery;
use crate::orchestrator::Orchestrator;

#[get("/post/{author_id}:{writ_id}")]
pub async fn render_post(
    id_parts: web::Path<(u64, u64)>,
    orc: web::Data<Arc<Orchestrator>>,
    req: HttpRequest,
) -> HttpResponse {
    let (author_id, writ_unique_id) = id_parts.into_inner();
    let writ_id = format!("post:{}:{}", author_id, writ_unique_id);

    let (o_usr, potential_renewal_cookie) = orc.user_by_session_renew(&req, Duration::days(3));

    let mut ctx = Context::new();
    if let Some(usr) = &o_usr {
        ctx.insert("username", &usr.username);
        ctx.insert("dev_mode", &orc.dev_mode);
    }

    let mut query = WritQuery::default();
    query.ids = Some(vec!(writ_id));
    query.public = Some(true);
    query.amount = Some(1);

    let public_writ = match orc.public_writ_query(query, o_usr) {
        Some(mut writs) => writs.pop().unwrap(),
        None => {
            return HttpResponse::NotFound()
                .content_type("text/plain")
                .body("We couldn't find any posts with that id.");
        },
    };

    ctx.insert("public_writ", &public_writ);

    match TEMPLATES.read().render("post.html", &ctx) {
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
                    .body(&format!("post.html template is broken - error : {}", err))
            } else {
                HttpResponse::InternalServerError()
                    .content_type("text/plain")
                    .body("The post page template is broken! :( We have failed you.")
            }
        }
    }
}
