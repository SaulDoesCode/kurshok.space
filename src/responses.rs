use actix_web::HttpResponse;
use serde::Serialize;

#[derive(Clone, Serialize, PartialEq, Debug)]
pub struct EmptyAPIResponse {
    pub ok: bool,
}

#[derive(Clone, Serialize, PartialEq, Debug)]
pub struct APIResponse<T: Serialize> {
    pub ok: bool,
    pub data: T,
}

#[derive(Clone, Serialize, PartialEq, Debug)]
pub struct APIStatusResponse<T: Serialize> {
    pub ok: bool,
    pub status: T,
}

#[derive(Clone, Serialize, PartialEq, Debug)]
pub struct APIStatusDataResponse<X: Serialize, Y: Serialize> {
    pub ok: bool,
    pub status: X,
    pub data: Y,
}

#[allow(non_snake_case, missing_docs)]
pub fn NotFoundEmpty() -> HttpResponse {
    HttpResponse::NotFound().json(EmptyAPIResponse { ok: false })
}

#[allow(non_snake_case, missing_docs)]
pub fn Ok<T: Serialize>(data: T) -> HttpResponse {
    HttpResponse::Ok().json(APIResponse { ok: true, data })
}

#[allow(non_snake_case, missing_docs)]
pub fn OkDataStatus<X: Serialize, Y: Serialize>(status: X, data: Y) -> HttpResponse {
    HttpResponse::Ok().json(APIStatusDataResponse {
        ok: true,
        data,
        status,
    })
}

#[allow(non_snake_case, missing_docs)]
pub fn AcceptedData<T: Serialize>(data: T) -> HttpResponse {
    HttpResponse::Accepted().json(APIResponse { ok: true, data })
}

#[allow(non_snake_case, missing_docs)]
pub fn AcceptedStatusData<X: Serialize, Y: Serialize>(status: X, data: Y) -> HttpResponse {
    HttpResponse::Accepted().json(APIStatusDataResponse {
        ok: true,
        data,
        status,
    })
}

#[allow(non_snake_case, missing_docs)]
pub fn Accepted<T: Serialize>(status: T) -> HttpResponse {
    HttpResponse::Accepted().json(APIStatusResponse { ok: true, status })
}

#[allow(non_snake_case, missing_docs)]
pub fn NotFound<T: Serialize>(status: T) -> HttpResponse {
    HttpResponse::NotFound().json(APIStatusResponse { ok: false, status })
}

#[allow(non_snake_case, missing_docs)]
pub fn BadRequest<T: Serialize>(status: T) -> HttpResponse {
    HttpResponse::BadRequest().json(APIStatusResponse { ok: false, status })
}

#[allow(non_snake_case, missing_docs)]
pub fn TooManyRequests<T: Serialize>(status: T) -> HttpResponse {
    HttpResponse::TooManyRequests().json(APIStatusResponse { ok: false, status })
}

#[allow(non_snake_case, missing_docs)]
pub fn Forbidden<T: Serialize>(status: T) -> HttpResponse {
    HttpResponse::Forbidden().json(APIStatusResponse { ok: false, status })
}

#[allow(non_snake_case, missing_docs)]
pub fn InternalServerError<T: Serialize>(status: T) -> HttpResponse {
    HttpResponse::Forbidden().json(APIStatusResponse { ok: false, status })
}
