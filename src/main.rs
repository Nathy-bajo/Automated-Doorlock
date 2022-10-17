extern crate mailgun_rs;

use crate::helpers::{polling, toggle_door_state};
use futures::StreamExt;
use prisma::{
    Door, DoorCreateInput, DoorWhereInput, DoorWhereInputId, FindFirstDoorArgs, FindFirstUserArgs,
    Prisma, User, UserCreateInput, UserWhereInput, UserWhereInputEmail,
};
use prisma_client::futures::lock::Mutex;
use rust_gpiozero::Servo;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tide::security::{CorsMiddleware, Origin};
use tide::{utils::After, Response, StatusCode};
use tide::{Body, Request};
use tide_websockets::{Message, WebSocket, WebSocketConnection};
use utils::Hasher;

mod auth;
pub mod controllers;
mod error;
pub mod helpers;
mod prisma;
mod utils;
use controllers::*;
mod videoroom;

#[derive(Deserialize, Serialize)]
pub struct ClaimsToken {
    pub kid: String,
    pub iss: String,
    pub iat: u64,
}

#[derive(serde::Deserialize)]
pub struct DataLoss {
    pub email: String,
}

#[derive(Deserialize, Serialize)]
pub struct Polling {
    pub door: String,
}

#[derive(Deserialize, Serialize)]
pub struct ResetPassword {
    pub email: String,
    pub reset_password: String,
}

#[derive(Deserialize, Serialize)]
pub struct Name {
    pub name: String,
}

#[derive(Deserialize, Serialize)]
pub struct FormData {
    pub email: String,
    pub current_password: String,
    pub new_password: String,
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

#[derive(Serialize, Deserialize)]
pub struct AppleNotifications {
    pub device_token: String,
}

#[derive(Serialize, Deserialize)]
pub struct NotificationMessage {
    pub aps: Alert,
}

#[derive(Serialize, Deserialize)]
pub struct Alert {
    pub alert: String,
}

pub type Result<T> = std::result::Result<T, error::Error>;

pub struct TideState {
    pub prisma: Prisma,
    pub servo: Arc<Mutex<Servo>>,
    // pub button: Arc<Mutex<Button>>,
    pub hasher: Hasher,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum DoorState {
    Open,
    Close,
}

impl core::fmt::Display for DoorState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = match self {
            DoorState::Open => "open",
            DoorState::Close => "close",
        };
        write!(f, "{}", state)
    }
}

impl DoorState {
    pub fn from_str(action: &str) -> std::result::Result<DoorState, ()> {
        let state = match action.to_lowercase().as_str() {
            "open" => DoorState::Open,
            "close" => DoorState::Close,
            _ => Err(())?,
        };
        Ok(state)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ButtonPress {
    Pressed,
}

#[derive(Deserialize, Serialize)]
pub struct ButtonPressed {
    pub button: String,
}

impl ButtonPress {
    pub fn from_str(button: &str) -> std::result::Result<ButtonPress, ()> {
        let state = match button.to_lowercase().as_str() {
            "pushed" => ButtonPress::Pressed,
            _ => Err(())?,
        };
        Ok(state)
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TransactionResponse {
    pub create_user: User,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct DoorResponse {
    pub create_door: Door,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let prisma = Prisma::new(vec![]).await?;
    let hasher = Hasher::new();
    let state = TideState {
        prisma,
        servo: Arc::new(Mutex::new(Servo::new(17))),
        hasher,
        // button: Arc::new(Mutex::new(
        //     Button::new(26).debounce(std::time::Duration::from_millis(100)),
        // )),
    };

    // let button_pressed = state.button.lock().await;
    // let mut button = button_pressed.wait_for_press(some(3));
    // create users

    let door_exist = state
        .prisma
        .first_door::<Door>(FindFirstDoorArgs {
            filter: Some(DoorWhereInput {
                id: Some(DoorWhereInputId::Int(1)),
                ..Default::default()
            }),
            ..Default::default()
        })
        .await?;
    if door_exist.is_none() {
        state
            .prisma
            .transaction()
            .create_door::<Door>(DoorCreateInput {
                state: DoorState::Open.to_string(),
            })?
            .execute::<DoorResponse>()
            .await?;
    }

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
                email: "seunlanlege@gmail.com".to_string(),
                name: "seun".to_string(),
                password: state.hasher.hash("Password@123")?,
                role: "admin".to_string(),
                device_token: None,
            })?
            .create_user::<User>(UserCreateInput {
                email: "example@gmail.com".to_string(),
                name: "nathaniel".to_string(),
                password: state.hasher.hash("Password@123")?,
                role: "admin".to_string(),
                device_token: None,
            })?
            .create_user::<User>(UserCreateInput {
                email: "oluwashinabajo@gmail.com".to_string(),
                name: "ayomide".to_string(),
                password: state.hasher.hash("Password@123")?,
                role: "admin".to_string(),
                device_token: None,
            })?
            .create_user::<User>(UserCreateInput {
                email: "jummyfola013@gmail.com".to_string(),
                name: "mum".to_string(),
                password: state.hasher.hash("Password@123")?,
                role: "admin".to_string(),
                device_token: None,
            })?
            .create_user::<User>(UserCreateInput {
                email: "debbiebajo@gmail.com".to_string(),
                name: "damilola".to_string(),
                password: state.hasher.hash("Password@123")?,
                role: "admin".to_string(),
                device_token: None,
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
        if let Some(err) = res.downcast_error::<tokio::io::Error>() {
            let msg = format!("Error: {:?}", err);
            res.set_status(StatusCode::NotFound);
            res.set_body(msg);
        }
        Ok(res)
    }));

    app.at("/login").post(login_handler);
    app.at("/reset").post(reset_handler);
    app.at("/door").post(toggle_door_state);
    app.at("/forgot").post(forgot_handler);
    app.at("/email").post(email_handler);
    app.at("/polling").get(polling);
    app.at("/notification").post(applenotification_handler);

    println!(r#"Server is running..."#);

    app.listen("0.0.0.0:8080").await?;

    Ok(())
}
