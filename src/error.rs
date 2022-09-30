use crate::prisma;
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
    #[error("jwt token not valid")]
    InternalServerError,
    #[error("internal server error")]
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
    Prisma(prisma::Error),
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
