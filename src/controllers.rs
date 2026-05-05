use crate::{
    auth::{create_jwt, Role},
    prisma::{
        FindFirstUserArgs, UpdateOneUserArgs, User, UserUpdateInput, UserUpdateInputDeviceToken,
        UserUpdateInputPassword, UserWhereInput, UserWhereInputEmail, UserWhereUniqueInput,
    },
    DataLoss, FormData, LoginResponse, ResetPassword,
};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use mailgun_rs::{EmailAddress, Mailgun, Message};
use std::{convert::TryInto, sync::Arc};
use tide::{Body, Error as TideError, Request, StatusCode};

use crate::{
    auth::{Claims, JWT_SECRET},
    error::Error,
    AppleNotifications, LoginRequest, TideState,
};

pub async fn applenotification_handler(mut req: Request<Arc<TideState>>) -> tide::Result {
    let notifications = req.body_json::<AppleNotifications>().await?;

    let token = req
        .header("Authorization")
        .map(|token| token.as_str().to_string());

    let token = token
        .map(|token| token.split("Bearer: ").collect::<Vec<_>>()[1].to_string())
        .ok_or_else(|| Error::JWTTokenError)?;

    let decoded = decode::<Claims>(
        &token,
        &DecodingKey::from_secret(JWT_SECRET),
        &Validation::new(Algorithm::HS512),
    )
    .map_err(|_e| {
        tide::http::Error::from_str(StatusCode::BadRequest, Error::JWTTokenError.to_string())
    })?;

    req.state()
        .prisma
        .update_user::<User>(UpdateOneUserArgs {
            data: UserUpdateInput {
                device_token: Some(Some(UserUpdateInputDeviceToken::String(
                    notifications.device_token,
                ))),
                ..Default::default()
            },
            filter: UserWhereUniqueInput {
                email: Some(decoded.claims.email),
                ..Default::default()
            },
        })
        .await
        .map_err(|e| TideError::from_str(400, format!("Invalid Token: {}", e)))?;

    Ok("Device token registered".into())
}

pub async fn login_handler(mut req: Request<Arc<TideState>>) -> tide::Result {
    let login_request = req.body_json::<LoginRequest>().await?;

    let password = login_request.pw;
    let user = req
        .state()
        .prisma
        .first_user::<User>(FindFirstUserArgs {
            filter: Some(UserWhereInput {
                email: Some(UserWhereInputEmail::String(login_request.email)),
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .map_err(|_e| TideError::from_str(StatusCode::NotFound, "User not found"))?
        .ok_or_else(|| Error::JWTTokenError)?;

    let matches = req
        .state()
        .hasher
        .verify(&password, &user.password)
        .map_err(|e| TideError::from_str(500, format!("Failed to verify password: {}", e)))?;

    if !matches {
        return Err(TideError::from_str(
            StatusCode::BadRequest,
            "email or password not correct",
        ));
    }

    let token = create_jwt(
        user.id.try_into().unwrap(),
        &Role::from_str(&user.role),
        user.email,
    )
    .map_err(tide::http::Error::from)?;

    let mut res = tide::Response::new(StatusCode::Accepted);
    res.set_body(Body::from_json(&LoginResponse { token }).unwrap());

    Ok(res)
}

pub async fn reset_handler(mut req: Request<Arc<TideState>>) -> tide::Result {
    let change_password = req.body_json::<FormData>().await?;

    let user = req
        .state()
        .prisma
        .first_user::<User>(FindFirstUserArgs {
            filter: Some(UserWhereInput {
                email: Some(UserWhereInputEmail::String(change_password.email)),
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .map_err(|_e| TideError::from_str(StatusCode::NotFound, "User not found"))?
        .ok_or_else(|| Error::JWTTokenError)?;

    let matches = req
        .state()
        .hasher
        .verify(&change_password.current_password, &user.password)
        .map_err(|e| TideError::from_str(300, format!("Failed to verify password: {}", e)))?;

    if !matches {
        return Err(TideError::from_str(
            StatusCode::BadRequest,
            "email or password not correct",
        ));
    }

    req.state()
        .prisma
        .update_user::<User>(UpdateOneUserArgs {
            data: UserUpdateInput {
                password: Some(UserUpdateInputPassword::String(
                    req.state()
                        .hasher
                        .hash(&change_password.new_password)
                        .map_err(|e| {
                            TideError::from_str(300, format!("Failed to hash password: {}", e))
                        })?,
                )),
                ..Default::default()
            },
            filter: UserWhereUniqueInput {
                email: Some(user.email),
                ..Default::default()
            },
        })
        .await
        .map_err(|e| TideError::from_str(400, format!("Password invalid: {}", e)))?;

    Ok("Password updated".into())
}

pub async fn email_handler(mut req: Request<Arc<TideState>>) -> tide::Result {
    let update_password = req.body_json::<ResetPassword>().await?;

    let user = req
        .state()
        .prisma
        .first_user::<User>(FindFirstUserArgs {
            filter: Some(UserWhereInput {
                email: Some(UserWhereInputEmail::String(update_password.email)),
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .map_err(|_e| TideError::from_str(StatusCode::NotFound, "User not found"))?
        .ok_or_else(|| Error::JWTTokenError)?;

    req.state()
        .prisma
        .update_user::<User>(UpdateOneUserArgs {
            data: UserUpdateInput {
                password: Some(UserUpdateInputPassword::String(
                    req.state()
                        .hasher
                        .hash(&update_password.reset_password)
                        .map_err(|e| {
                            TideError::from_str(400, format!("Failed to hash password: {}", e))
                        })?,
                )),
                ..Default::default()
            },
            filter: UserWhereUniqueInput {
                email: Some(user.email),
                ..Default::default()
            },
        })
        .await
        .map_err(|e| TideError::from_str(400, format!("Password invalid: {}", e)))?;

    Ok("Password reset".into())
}

pub fn decode_token(token: Option<String>) -> std::result::Result<String, tide::http::Error> {
    let token = token.map(|token| token.split("Bearer: ").collect::<Vec<_>>()[1].to_string());

    match token {
        Some(jwt) => {
            let decoded = decode::<Claims>(
                &jwt,
                &DecodingKey::from_secret(JWT_SECRET),
                &Validation::new(Algorithm::HS512),
            )
            .map_err(|e| {
                println!("Token decode error: {:?}", e);
                tide::http::Error::from_str(StatusCode::BadRequest, Error::JWTTokenError)
            })?;
            if decoded.claims.role != Role::Admin {
                Err(tide::http::Error::from_str(
                    StatusCode::BadRequest,
                    Error::NoPermissionError,
                ))
            } else {
                Ok(decoded.claims.email)
            }
        }
        None => Err(tide::http::Error::from_str(
            StatusCode::BadRequest,
            "Server error",
        )),
    }
}

pub async fn forgot_handler(mut req: Request<Arc<TideState>>) -> tide::Result {
    let forgot_password = req.body_json::<DataLoss>().await?;
    let user = req
        .state()
        .prisma
        .first_user::<User>(FindFirstUserArgs {
            filter: Some(UserWhereInput {
                email: Some(UserWhereInputEmail::String(forgot_password.email)),
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .ok()
        .flatten()
        .ok_or_else(|| TideError::from_str(StatusCode::NotFound, "User not found"))?;

    let token = create_jwt(
        user.id as usize,
        &Role::from_str(&user.role.clone()),
        user.email.clone(),
    )
    .map_err(tide::http::Error::from)?;

    let domain = "sandbox3234fec2e6144717bf98ddfca5eb0b81.mailgun.org";
    let key = "02c914953aae6aef71afd139f07d4a06-02fa25a3-25b8c2b9";
    let recipient = EmailAddress::address(&user.email);
    let message = Message {
        to: vec![recipient],
        subject: String::from("Change your password here"),
        text: String::from("Are you ready to change your password"),
        html: format!(
            "<p><a href=\"http://192.168.100.204:3000/email?token={}\">click to reset password</a></p>",
            token
        ),
        ..Default::default()
    };

    let client = Mailgun {
        api_key: String::from(key),
        domain: String::from(domain),
        message,
    };
    let sender = EmailAddress::name_address(
        "Click to change your password",
        "postmaster@sandbox3234fec2e6144717bf98ddfca5eb0b81.mailgun.org",
    );

    if let Err(err) = client.send(&sender) {
        println!("Mailgun send error: {}", err);
    }

    let mut res = tide::Response::new(StatusCode::Accepted);
    res.set_body(Body::from_json(&LoginResponse { token }).unwrap());
    Ok(res)
}
