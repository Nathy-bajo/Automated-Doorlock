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

    let test_token = req
        .state()
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
    println!("apple token? {:?}", test_token);
    println!("well");

    Ok(format!("Tester ",).into())
}

pub async fn login_handler(mut req: Request<Arc<TideState>>) -> tide::Result {
    let login_request = req.body_json::<LoginRequest>().await?;

    let password = login_request.pw; // bcrypt
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
        .ok_or_else(|| Error::JWTTokenError)?; //tide error
    println!("{}", user.password);
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
    .map_err(|e| tide::http::Error::from(e))?;

    println!("token: {}", token);

    let mut res = tide::Response::new(StatusCode::Accepted);
    res.set_body(Body::from_json(&LoginResponse { token }).unwrap());

    Ok(res)
}

pub async fn reset_handler(mut req: Request<Arc<TideState>>) -> tide::Result {
    let change_password = req.body_json::<FormData>().await?;
    println!("crazy");
    // check token validity

    // let token = req
    //     .header("Authorization")
    //     .map(|token| token.as_str().to_string());
    // let email = decode_token(token)?;

    // fetch user with id from token
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
        .ok_or_else(|| Error::JWTTokenError)?; //tide error
    println!("Works");
    // hash the old password in FormData, compare with the user password from db
    let matches = req
        .state()
        .hasher
        .verify(&change_password.current_password, &user.password)
        .map_err(|e| TideError::from_str(300, format!("Failed to verify password: {}", e)))?;
    println!("Still works");

    if !matches {
        return Err(TideError::from_str(
            StatusCode::BadRequest,
            "email or password not correct",
        ));
    }

    // take the hash of new password and update user in db
    println!("still working");
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
    println!("well");

    Ok(format!("Hello User ",).into())
}

pub async fn email_handler(mut req: Request<Arc<TideState>>) -> tide::Result {
    let update_password = req.body_json::<ResetPassword>().await?;
    println!("Sasageyo!");

    // let token = req
    //     .header("Authorization")
    //     .map(|token| token.as_str().to_string());

    // let emails = decode_token(token)?;

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
        .ok_or_else(|| Error::JWTTokenError)?; //tide error
    println!("Sassyy");

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
    println!("Partss");

    Ok(format!("Hello User ",).into())
}

pub fn decode_token(token: Option<String>) -> std::result::Result<String, tide::http::Error> { 
    let token = token.map(|token| token.split("Bearer: ").collect::<Vec<_>>()[1].to_string());

    println!("Common");

    match token {
        Some(jwt) => {
            let decoded = decode::<Claims>(
                &jwt,
                &DecodingKey::from_secret(JWT_SECRET),
                &Validation::new(Algorithm::HS512),
            )
            .map_err(|_e| {
                println!("Token Decode Error {:?}", _e);
                tide::http::Error::from_str(StatusCode::BadRequest, Error::JWTTokenError)
            })?;
            if decoded.claims.role != Role::Admin {
                Err(tide::http::Error::from_str(
                    StatusCode::BadRequest,
                    Error::NoPermissionError,
                ))?
            } else {
                Ok(decoded.claims.email)
            }
        }
        None => Err(tide::http::Error::from_str(
            StatusCode::BadRequest,
            "Server error",
        ))?,
    }

    // println!("GAs");
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
        .ok_or(TideError::from_str(StatusCode::NotFound, "User not found"))?; //tide error
    println!("Workss");

    let token = create_jwt(
        user.id as usize,
        &Role::from_str(&user.role.clone()),
        user.email.clone(),
    )
    .map_err(|e| tide::http::Error::from(e))?;
    println!("This is my token={}", token);

    let domain = "sandbox3234fec2e6144717bf98ddfca5eb0b81.mailgun.org";
    let key = "02c914953aae6aef71afd139f07d4a06-02fa25a3-25b8c2b9";
    let recipient = user.email;
    let recipient = EmailAddress::address(&recipient);
    let message = Message {
        to: vec![recipient],
        subject: String::from("Change your password here"),
        text: String::from("Are you ready to change your password"),
        html: format!("<p><a href=\"http://192.168.100.204:3000/email?token={}\">click to reset password</a></p>", token),
        ..Default::default()
    };
    println!("yupp still workss");

    let client = Mailgun {
        api_key: String::from(key),
        domain: String::from(domain),
        message,
    };
    let sender = EmailAddress::name_address(
        "Click to change your password",
        "postmaster@sandbox3234fec2e6144717bf98ddfca5eb0b81.mailgun.org",
    );

    match client.send(&sender) {
        Ok(_) => {
            println!("successful");
        }
        Err(err) => {
            println!("{}", err);
        }
    }

    println!("Hehehe");

    let mut res = tide::Response::new(StatusCode::Accepted);
    res.set_body(Body::from_json(&LoginResponse { token }).unwrap());
    Ok(res)

    // Ok(format!("Hello User ",).into())
}
