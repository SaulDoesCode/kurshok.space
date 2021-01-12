use actix_web::{get, web, HttpRequest, HttpResponse};
use tera::Context;
use time::Duration;

use super::TEMPLATES;

use crate::{
    orchestrator::ORC,
//  utils::FancyIVec,
    writs::{
        WritID,
        WritQuery
    }
};

#[get("/post/{author_id}:{writ_id}")]
pub async fn render_post(
    id_parts: web::Path<(u64, u64)>,
    req: HttpRequest,
) -> HttpResponse {
    let (author_id, writ_unique_id) = id_parts.into_inner();
    let writ_id = format!("post:{}:{}", author_id, writ_unique_id);

    let (o_usr, potential_renewal_cookie) = ORC.user_by_session_renew(&req, Duration::days(3));

    let mut ctx = Context::new();
    if let Some(usr) = &o_usr {
        ctx.insert("username", &usr.username);
        ctx.insert("dev_mode", &ORC.dev_mode);
    }

    let mut query = WritQuery::default();
    query.ids = Some(vec![writ_id]);
    query.public = Some(true);
    query.amount = Some(1);

    let public_writ = match ORC.public_writ_query(query, o_usr.as_ref()) {
        Some(mut writs) => writs.pop().unwrap(),
        None => {
            return render_404(
                &mut ctx,
                "We couldn't find any posts with that id.",
                ORC.dev_mode,
            )
        }
    };

    ctx.insert("public_writ", &public_writ);

    let mut res = HttpResponse::Ok();
    render_template(
        &mut ctx,
        "post.html",
        match potential_renewal_cookie {
            Some(c) => res.cookie(c),
            None => &mut res,
        },
        ORC.dev_mode,
    )
}

#[get("/post/{slug}")]
pub async fn render_post_by_slug(
    slug: web::Path<String>,
    req: HttpRequest,
) -> HttpResponse {
    let mut ctx = Context::new();

    let slug_key = format!("post:{}", slug.into_inner());
    let writ_id = if let Ok(Some(writ_id)) = ORC.slugs.get(slug_key.as_bytes()) {
        WritID::from_bin(&writ_id).to_string()
    } else {
        return render_404(
            &mut ctx,
            "That's a bad slug, we couldn't find any posts matching it.",
            ORC.dev_mode,
        );
    };

    let (o_usr, potential_renewal_cookie) = ORC.user_by_session_renew(&req, Duration::days(3));

    if let Some(usr) = &o_usr {
        ctx.insert("user", usr);
    }

    let mut query = WritQuery::default();
    query.ids = Some(vec![writ_id]);
    query.public = Some(true);
    query.amount = Some(1);

    let public_writ = match ORC.public_writ_query(query, o_usr.as_ref()) {
        Some(mut writs) => writs.pop().unwrap(),
        None => {
            return render_404(
                &mut ctx,
                "That's a bad slug, we couldn't find any posts matching it.",
                ORC.dev_mode,
            );
        }
    };

    ctx.insert("public_writ", &public_writ);

    let mut res = HttpResponse::Ok();
    render_template(
        &mut ctx,
        "post.html",
        match potential_renewal_cookie {
            Some(c) => res.cookie(c),
            None => &mut res,
        },
        ORC.dev_mode,
    )
}

fn render_404(ctx: &mut Context, message: &str, dev_mode: bool) -> HttpResponse {
    ctx.insert("message", &message);
    ctx.insert("dev_mode", &dev_mode);
    match TEMPLATES.read().render("404.html", &ctx) {
        Ok(s) => HttpResponse::NotFound().content_type("text/html").body(s),
        Err(err) => {
            if dev_mode {
                HttpResponse::InternalServerError()
                    .content_type("text/plain")
                    .body(&format!("404.html template is broken - error : {}", err))
            } else {
                HttpResponse::InternalServerError()
                    .content_type("text/plain")
                    .body("The bloody 404.html template is broken! :( We have failed you.")
            }
        }
    }
}

fn render_template(
    ctx: &mut Context,
    name: &str,
    res: &mut actix_web::dev::HttpResponseBuilder,
    dev_mode: bool,
) -> HttpResponse {
    ctx.insert("dev_mode", &dev_mode);
    match TEMPLATES.read().render(name, &ctx) {
        Ok(s) => res.content_type("text/html").body(s),
        Err(err) => {
            if dev_mode {
                HttpResponse::InternalServerError()
                    .content_type("text/plain")
                    .body(&format!("{} template is broken - error : {}", name, err))
            } else {
                HttpResponse::InternalServerError()
                    .content_type("text/plain")
                    .body(&format!(
                        "The bloody {} template is broken or missing! :( We have failed you.",
                        name
                    ))
            }
        }
    }
}
