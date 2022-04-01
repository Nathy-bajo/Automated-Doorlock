use crate::{
    error::{self, Error},
    Result,
};
use chrono::Utc;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::fmt;
use tide::{
    http::{headers::AUTHORIZATION, Headers},
    StatusCode,
};

#[allow(dead_code)]
async fn authorize((role, headers): (Role, Headers)) -> Result<usize> {
    match jwt_from_header(headers) {
        Ok(jwt) => {
            let decoded = decode::<Claims>(
                &jwt,
                &DecodingKey::from_secret(JWT_SECRET),
                &Validation::new(Algorithm::HS512),
            )
            .map_err(|_| {
                tide::http::Error::from_str(
                    StatusCode::BadRequest,
                    Error::JWTTokenError.to_string(),
                )
            })?;

            if role == Role::Admin && decoded.claims.role != Role::Admin {
                return Err(error::Error::Tide(tide::http::Error::from_str(
                    StatusCode::BadRequest,
                    Error::NoPermissionError.to_string(),
                )));
            }

            Ok(decoded.claims.sub)
        }
        Err(e) => return Err(e.into()),
    }
}

#[allow(dead_code)]
fn jwt_from_header(headers: Headers) -> Result<String> {
    let header = match headers.get(AUTHORIZATION) {
        Some(v) => v,
        None => return Err(Error::NoAuthHeaderError),
    };
    let auth_header = header.to_string();
    if !auth_header.starts_with(BEARER) {
        return Err(Error::InavlidAuthHeaderError);
    }
    Ok(auth_header.trim_start_matches(BEARER).to_owned())
}

pub fn create_jwt(uid: usize, role: &Role, email: String) -> Result<String> {
    let expiration = Utc::now()
        .checked_add_signed(chrono::Duration::minutes(30))
        .expect("valid timestamp")
        .timestamp();

    let claims = Claims {
        sub: uid,
        role: role.clone(),
        exp: expiration as usize,
        email,
    };

    let header = Header::new(Algorithm::HS512);
    encode(&header, &claims, &EncodingKey::from_secret(JWT_SECRET))
        .map_err(|_| Error::JWTTokenCreationError)
}

#[allow(dead_code)]
const BEARER: &str = "Bearer ";
pub const JWT_SECRET: &[u8] = b"secret";
pub const SALT: [u8; 32] = [
    80, 166, 186, 240, 120, 151, 49, 155, 56, 42, 67, 81, 68, 108, 7, 35, 136, 46, 20, 235, 238,
    17, 110, 94, 225, 101, 181, 63, 70, 216, 100, 236,
];

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub enum Role {
    User,
    Admin,
}

impl Role {
    pub fn from_str(role: &str) -> Role {
        match role {
            "admin" => Role::Admin,
            _ => Role::User,
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Role::User => write!(f, "User"),
            Role::Admin => write!(f, "Admin"),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Claims {
    pub sub: usize,
    pub role: Role,
    pub exp: usize,
    pub email: String,
}
