use crate::auth::{Claims, Role, JWT_SECRET};
extern crate mailgun_rs;
use crate::error::Error;
use auth::create_jwt;
use mailgun_rs::{EmailAddress, Mailgun, Message};
// use chrono::{offset, DateTime, NaiveDateTime, TimeZone, Utc};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use prisma_client::{
    FindFirstUserArgs, Prisma, UpdateOneUserArgs, User, UserCreateInput, UserUpdateInput,
    UserWhereInput, UserWhereInputEmail, UserWhereUniqueInput,
};
// use rust_gpiozero::Servo;
use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use std::sync::Arc;
// use std::sync::Mutex;
use tide::security::{CorsMiddleware, Origin};
use tide::{utils::After, Body, Error as TideError, Request, Response, StatusCode};
use utils::Hasher;

mod auth;
mod error;
mod utils;

#[derive(serde::Deserialize)]
pub struct DataLoss {
    pub email: String,
}

#[derive(serde::Deserialize)]
pub struct ResetPassword {
    email: String,
    reset_password: String,
    // confirm_password: String,
}

#[derive(serde::Deserialize)]
pub struct FormData {
    pub email: String,
    pub current_password: String,
    pub new_password: String,
    // new_password_check: Secret<String>,
}

#[derive(Deserialize, Serialize)]
pub struct LoginRequest {
    pub email: String,
    pub pw: String,
}

#[derive(Deserialize, Serialize)]
pub struct AdminRequest {
    pub action: String,
}

#[derive(Serialize, Deserialize)]
pub struct LoginResponse {
    pub token: String,
}

type Result<T> = std::result::Result<T, error::Error>;

pub struct State {
    pub prisma: Prisma,
    // pub servo: Mutex<Servo>,
    pub hasher: Hasher,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum Action {
    Open,
    Close,
    Default,
}

impl Action {
    pub fn from_str(action: &str) -> Action {
        match action {
            "close" => Action::Open,
            "open" => Action::Close,
            _ => Action::Default,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct TransactionResponse {
    create_user: User,
}

#[async_std::main]
async fn main() -> Result<()> {
    let prisma = Prisma::new(vec![]).await?;
    let hasher = Hasher::new();
    let state = State {
        prisma,
        // servo: Mutex::new(Servo::new(17)),
        hasher,
    };

    // create users
    let users_exist = state
        .prisma
        .first_user::<User>(FindFirstUserArgs {
            filter: Some(UserWhereInput {
                email: Some(UserWhereInputEmail::String(
                    "seunlanlege@gmail.com".to_string(),
                )),
                ..Default::default()
            }),
            ..Default::default()
        })
        .await?;
    if users_exist.is_none() {
        state
            .prisma
            .transaction()
            .create_user::<User>(UserCreateInput {
                email: "example@gmail.com".to_string(),
                name: "seun".to_string(),
                password: state.hasher.hash("Password@123")?,
                role: "admin".to_string(),
            })?
            .create_user::<User>(UserCreateInput {
                email: "bajon7680@gmail.com".to_string(),
                name: "nathaniel".to_string(),
                password: state.hasher.hash("Password@123")?,
                role: "admin".to_string(),
            })?
            .create_user::<User>(UserCreateInput {
                email: "oluwashinabajo@gmail.com".to_string(),
                name: "ayomide".to_string(),
                password: state.hasher.hash("Password@123")?,
                role: "admin".to_string(),
            })?
            .create_user::<User>(UserCreateInput {
                email: "jummyfola013@gmail.com".to_string(),
                name: "mum".to_string(),
                password: state.hasher.hash("Password@123")?,
                role: "admin".to_string(),
            })?
            .create_user::<User>(UserCreateInput {
                email: "debbiebajo@gmail.com".to_string(),
                name: "damilola".to_string(),
                password: state.hasher.hash("Password@123")?,
                role: "admin".to_string(),
            })?
            .execute::<TransactionResponse>()
            .await?;
    }

    let cors = CorsMiddleware::new()
        .allow_methods(
            "Get, POST, OPTIONS"
                .parse::<tide::http::headers::HeaderValue>()
                .unwrap(),
        )
        .allow_origin(Origin::from("*"))
        .allow_credentials(false);

    let mut app = tide::with_state(Arc::new(state));

    app.with(cors);

    app.with(After(|mut res: Response| async {
        if let Some(err) = res.downcast_error::<async_std::io::Error>() {
            let msg = format!("Error: {:?}", err);
            res.set_status(StatusCode::NotFound);
            res.set_body(msg);
        }
        Ok(res)
    }));

    app.at("/login").post(login_handler);
    app.at("/reset").post(reset_handler);
    app.at("/door").post(admin_handler);
    app.at("/forgot").post(forgot_handler);
    app.at("/email").post(email_handler);

    println!(r#"Server is running..."#);

    app.listen("0.0.0.0:8080").await?;

    Ok(())
}

pub async fn login_handler(mut req: Request<Arc<State>>) -> tide::Result {
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

    let mut res = tide::Response::new(StatusCode::Accepted);
    res.set_body(Body::from_json(&LoginResponse { token }).unwrap());
    Ok(res)
}

pub async fn reset_handler(mut req: Request<Arc<State>>) -> tide::Result {
    let change_password = req.body_json::<FormData>().await?;
    println!("Testing");
    // check token validity

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
                password: Some(prisma_client::UserUpdateInputPassword::String(
                    req.state()
                        .hasher
                        .hash(&change_password.new_password)
                        .map_err(|e| {
                            TideError::from_str(300, format!("Failed to hash password: {}", e))
                        })?,
                )),
                ..Default::default()
            },
            // filter: Default::default()
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

pub async fn admin_handler(mut req: Request<Arc<State>>) -> tide::Result {
    // check auth

    let token = req
        .header("Authorization")
        .map(|token| token.as_str().to_string());

    let token = token.map(|token| token.split("Bearer: ").collect::<Vec<_>>()[1].to_string());

    let (action, email) = match token {
        Some(jwt) => {
            let decoded = decode::<Claims>(
                &jwt,
                &DecodingKey::from_secret(JWT_SECRET),
                &Validation::new(Algorithm::HS512),
            )
            .map_err(|_e| {
                tide::http::Error::from_str(
                    StatusCode::BadRequest,
                    Error::JWTTokenError.to_string(),
                )
            })?;
            if decoded.claims.role != Role::Admin {
                return Err(tide::http::Error::from_str(
                    StatusCode::BadRequest,
                    Error::NoPermissionError.to_string(),
                ));
            } else {
                let request = req.body_json::<AdminRequest>().await?;
                (request.action, decoded.claims.email)
            }
        }
        None => {
            return Err(tide::http::Error::from_str(
                StatusCode::BadRequest,
                "Server error",
            ))
        }
    };
    let log = format!("{},{},{}", email, chrono::Utc::now(), action);
    let _ = std::fs::write("log.txt", log.as_bytes());

    // put this in state
    // let servo = &req.state().servo;
    // {
    //     if let Ok(mut servo) = servo.lock() {
    //         match Action::from_str(&action) {
    //             Action::Open => {
    //                 servo.min();
    //             }
    //             Action::Close => {
    //                 servo.max();
    //             }
    //             Action::Default => {}
    //         }
    //     };
    // };

    Ok(format!("Action executed").into())
}

async fn forgot_handler(mut req: Request<Arc<State>>) -> tide::Result {
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

async fn email_handler(mut req: Request<Arc<State>>) -> tide::Result {
    let update_password = req.body_json::<ResetPassword>().await?;
    println!("Sasageyo!");

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
                password: Some(prisma_client::UserUpdateInputPassword::String(
                    req.state()
                        .hasher
                        .hash(&update_password.reset_password)
                        .map_err(|e| {
                            TideError::from_str(300, format!("Failed to hash password: {}", e))
                        })?,
                )),
                ..Default::default()
            },
            // filter: Default::default()
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
