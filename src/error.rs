use derive_more::From;
use serde::Serialize;
use thiserror::Error;

#[allow(dead_code)]
#[derive(Error, Debug, From)]
pub enum Error {
    #[error("wrong credentials")]
    WrongCredentialsError,
    #[error("jwt token not valid")]
    JWTTokenError,
    #[error("jwt token creation error")]
    JWTTokenCreationError,
    #[error("no auth header")]
    NoAuthHeaderError,
    #[error("invalid auth header")]
    InavlidAuthHeaderError,
    #[error("no permission")]
    NoPermissionError,
    #[error("io error {:?}", _0)]
    IoError(std::io::Error),
    #[error("parse int error {:?}", _0)]
    ParseError(std::num::ParseIntError),
    #[error("prisma client error {:?}", _0)]
    Prisma(prisma_client::Error),
    #[error("tide error {:?}", _0)]
    Tide(tide::Error),
    #[error("argon error {:?}", _0)]
    Argon(argon2::Error),
}

#[derive(Serialize, Debug)]
struct ErrorResponse {
    message: String,
    status: String,
}

// pub async fn handle_rejection(err: Rejection) -> std::result::Result<impl Reply, Infallible> {
//     let (code, message) = if err.is_not_found() {
//         (StatusCode::NOT_FOUND, "Not Found".to_string())
//     } else if let Some(e) = err.find::<Error>() {
//         match e {
//             Error::WrongCredentialsError => (StatusCode::FORBIDDEN, e.to_string()),
//             Error::NoPermissionError => (StatusCode::UNAUTHORIZED, e.to_string()),
//             Error::JWTTokenError => (StatusCode::UNAUTHORIZED, e.to_string()),
//             Error::JWTTokenCreationError => (
//             StatusCode::INTERNAL_SERVER_ERROR,
//             "Internal Server Error".to_string(),
//             ),
//             _=> (StatusCode::BAD_REQUEST, e.to_string())
//         }
//     } else if err.find::<warp::reject::MethodNotAllowed>().is_some() {
//         (
//             StatusCode::METHOD_NOT_ALLOWED,
//             "Method Not Allowed".to_string(),
//         )
//     } else {
//         eprintln!("unhandled error: {:?}", err);
//         (
//             StatusCode::INTERNAL_SERVER_ERROR,
//             "Internal Server Error".to_string(),
//         )
//     };

//     let json = warp::reply::json(&ErrorResponse {
//         status: code.to_string(),
//         message,
//     });
//     Ok(warp::reply::with_status(json, code))
// }

