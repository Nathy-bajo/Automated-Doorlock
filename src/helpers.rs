use crate::{
    prisma::{
        Door, DoorUpdateInput, DoorUpdateInputState, DoorWhereUniqueInput, FindFirstUserArgs,
        FindManyUserArgs, UpdateOneDoorArgs, User, UserWhereInput, UserWhereInputEmail,
    },
    ButtonPress, Polling,
};
// use chrono::Utc;
// use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use std::sync::Arc;
use tide::{Error as TideError, Request, StatusCode};
use tide_websockets::WebSocketConnection;

// pub async fn index(req: Request<Arc<TideState>>, wsc: WebSocketConnection) -> tide::Result {
//     use crate::videoroom::ws_index;

//     ws_index(req, wsc).await?;

//     Ok(format!("Action executed").into())
// }

pub async fn polling(req: Request<Arc<TideState>>) -> tide::Result {
    let token = req
        .header("Authorization")
        .map(|token| token.as_str().to_string());

    let _ = decode_token(token)?;

    let door = req
        .state()
        .prisma
        .door::<Door>(DoorWhereUniqueInput { id: Some(1) })
        .await
        .map_err(|_e| {
            tide::http::Error::from_str(StatusCode::BadRequest, Error::InavlidAuthHeaderError)
        })?
        .unwrap();

    println!("doorstate: {:?}", door);

    let mut res = tide::Response::new(StatusCode::Accepted);
    res.set_body(tide::Body::from_json(&Polling { door: door.state }).unwrap());

    Ok(res)
}

async fn button(mut req: Request<Arc<TideState>>) -> tide::Result {
    let request = req.body_json::<crate::ButtonPressed>().await?;

    // let button = req.state().button;
    // let mut button = button.lock().await;

    let button_state = match ButtonPress::from_str(&request.button).map_err(|_e| {
        tide::http::Error::from_str(StatusCode::NotAcceptable, Error::NoAuthHeaderError)
    })? {
        ButtonPress::Pressed => {
            // button.wait_for_press(None);
            ButtonPress::Pressed
        }
    };

    let message = match button_state {
        ButtonPress::Pressed => format!(
            "{:?}Someone is waiting for you at the door! http://192.168.100.6:3000/video",
            button_state
        ),
    };

    // .when_pressed(|_| {
    //     println!("button pressed");
    // })
    // .unwrap();

    notification_handler(req.state().clone(), message).await?;

    Ok(format!("Action executed").into())

    // let mut button = button.lock().await?;
}

pub async fn toggle_door_state(req: Request<Arc<TideState>>) -> tide::Result {
    // check auth
    let token = req
        .header("Authorization")
        .map(|token| token.as_str().to_string());

    let email = decode_token(token)?;

    let user = req
        .state()
        .prisma
        .first_user::<User>(FindFirstUserArgs {
            filter: Some(UserWhereInput {
                email: Some(UserWhereInputEmail::String(email.clone())),
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .map_err(|_e| TideError::from_str(StatusCode::NotFound, "User not found"))?
        .ok_or_else(|| Error::JWTTokenError)?;

    // first check state of the door
    let door = req
        .state()
        .prisma
        .door::<Door>(DoorWhereUniqueInput { id: Some(1) })
        .await
        .map_err(|_e| {
            tide::http::Error::from_str(StatusCode::BadRequest, Error::InavlidAuthHeaderError)
        })?
        .unwrap();

    println!("Toggle state:{:?}", door);

    // let request = req.body_json::<crate::AdminRequest>().await?;
    // let doors = DoorState::from_str(&request.action).map_err(|_e| {
    //     tide::http::Error::from_str(StatusCode::NotAcceptable, Error::NoAuthHeaderError)
    // })?;

    let log = format!("{},{},{}", email, chrono::Utc::now(), &door.state);
    let _ = std::fs::write("log.txt", log.as_bytes());

    println!("{}", log);

    // put this in state
    let servo = &req.state().servo;
    let mut servo = servo.lock().await;

    let door_state = match DoorState::from_str(&door.state).map_err(|_e| {
        tide::http::Error::from_str(StatusCode::NotAcceptable, Error::NoAuthHeaderError)
    })? {
        DoorState::Open => {
            servo.min();
            DoorState::Close
        }
        DoorState::Close => {
            servo.max();
            DoorState::Open
        }
    };

    let testdoor = req
        .state()
        .prisma
        .update_door::<Door>(UpdateOneDoorArgs {
            data: DoorUpdateInput {
                state: Some(DoorUpdateInputState::String(door_state.to_string())),
            },
            filter: DoorWhereUniqueInput { id: Some(1) },
        })
        .await
        .map_err(|e| TideError::from_str(400, format!("Action invalid: {}", e)))?;

    println!("Update: {:?}", testdoor);

    let message = match door_state {
        DoorState::Open => {
            format!("{} {}ed the door", user.name, door_state)
        }
        DoorState::Close => {
            format!("{} {}d the door", user.name, door_state)
        }
    };

    notification_handler(req.state().clone(), message).await?;

    Ok(format!("Action executed").into())
}

use a2::{Client, Endpoint, NotificationBuilder, NotificationOptions, PlainNotificationBuilder};
use argparse::{ArgumentParser, Store, StoreOption, StoreTrue};

async fn notification_handler(state: Arc<TideState>, message: String) -> tide::Result {
    // pretty_env_logger::init();

    let users = state
        .prisma
        .users::<User>(FindManyUserArgs {
            ..Default::default()
        })
        .await
        .map_err(|e| TideError::from_str(400, format!("Error occured: {}", e)))?;

    let tokens = users
        .into_iter()
        .filter_map(|user| user.device_token)
        .collect::<Vec<_>>();

    println!("apple token: {:?}", tokens);

    for mut device_token in tokens {
        let mut key_file = "AuthKey_6DRGKC394S.p8".to_string();
        let mut team_id = "Z589Y98374".to_string();
        let mut key_id = "6DRGKC394S".to_string();
        let mut message = message.clone();
        let mut sandbox = true;
        let mut topic: Option<String> = Some("com.nathanielbajo.loginForm".to_string());
        println!("server");

        {
            let mut ap = ArgumentParser::new();
            ap.set_description("APNs token-based push");
            ap.refer(&mut key_file)
                .add_option(&["-p", "--pkcs8"], Store, "Private key PKCS8");
            ap.refer(&mut team_id)
                .add_option(&["-t", "--team_id"], Store, "APNs team ID");
            ap.refer(&mut key_id)
                .add_option(&["-k", "--key_id"], Store, "APNs key ID");
            ap.refer(&mut device_token).add_option(
                &["-d", "--device_token"],
                Store,
                "APNs device token",
            );
            ap.refer(&mut message)
                .add_option(&["-m", "--message"], Store, "Notification message");
            ap.refer(&mut sandbox).add_option(
                &["-s", "--sandbox"],
                StoreTrue,
                "Use the development APNs servers",
            );
            ap.refer(&mut topic)
                .add_option(&["-o", "--topic"], StoreOption, "APNS topic");
            ap.parse_args_or_exit();
        }

        println!("server2");

        let mut private_key = std::fs::File::open(key_file).unwrap();

        let endpoint = if sandbox {
            Endpoint::Sandbox
        } else {
            Endpoint::Production
        };

        println!("server3");

        let client = Client::token(&mut private_key, key_id, team_id, endpoint).unwrap();

        let options = NotificationOptions {
            apns_topic: topic.as_ref().map(|s| &**s),
            ..Default::default()
        };

        println!("server4");

        let mut builder = PlainNotificationBuilder::new(message.as_ref());
        builder.set_sound("default");
        builder.set_badge(1u32);

        println!("server5");

        let payload = builder.build(device_token.as_ref(), options);
        println!("server6");

        let response = client.send(payload).await.map_err(|_e| {
            println!("{}", _e);
            tide::http::Error::from_str(StatusCode::BadRequest, Error::InavlidAuthHeaderError)
        })?;
        println!("server7");

        println!("apple notification server {:?}", response);
    }

    Ok(format!("Testing ").into())
}

async fn nnotifications_handler(
    action: DoorState,
    email: String,
    state: Request<Arc<TideState>>,
) -> tide::Result {
    let header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::ES256);

    println!("excuse");

    let claims = ClaimsToken {
        kid: "6DRGKC394S".to_string(),
        iss: "Z589Y98374".to_string(),
        iat: {
            chrono::Utc::now()
                .checked_add_signed(chrono::Duration::minutes(5))
                .expect("valid timestamp")
                .timestamp() as u64
        },
    };

    println!("comes");
    let secret = std::fs::read("AuthKey_6DRGKC394S.p8").unwrap();

    let auth_token = jsonwebtoken::encode(
        &header,
        &claims,
        &jsonwebtoken::EncodingKey::from_ec_pem(&secret).unwrap(),
    )
    .map_err(|_e| {
        println!("{}", _e);
        tide::http::Error::from_str(StatusCode::NotAcceptable, Error::InavlidAuthHeaderError)
    })?;

    // let final_token = decode::<ClaimsToken>();

    let users = state
        .state()
        .prisma
        .users::<User>(FindManyUserArgs {
            ..Default::default()
        })
        .await
        .map_err(|e| TideError::from_str(400, format!("Error occured: {}", e)))?;

    println!("casted");

    let tokens = users
        .into_iter()
        .filter_map(|user| user.device_token)
        .collect::<Vec<_>>();

    println!("KISAMA");

    for token in tokens {
        let mut headers = reqwest::header::HeaderMap::new();
        println!("vars");
        headers.insert("path", format!("/3/device/{:?}", token).parse().unwrap());
        headers.insert(
            "Authorization",
            format!("bearer {:?}", auth_token).parse().unwrap(),
        );
        headers.insert(
            "apns-id",
            format!("{}", "94b6288c72adf7eb6579eeed67cee1cc52be1bd272098d1e77ce7aa8edee32c7")
                .parse()
                .unwrap(),
        );
        headers.insert("apns-push-type", format!("{}", "alert").parse().unwrap());
        headers.insert("apns-expiration", format!("{}", 0).parse().unwrap());
        headers.insert("apns-priority", format!("{}", 10).parse().unwrap());
        headers.insert(
            "apns-topic",
            format!("{}", "com.nathanielbajo.loginForm")
                .parse()
                .unwrap(),
        );

        println!("caoep");

        let body = NotificationMessage {
            aps: Alert {
                alert: match action {
                    DoorState::Open => {
                        format!("{} opened the door {}", email, action)
                    }
                    DoorState::Close => {
                        format!("{} closed the door {}", email, action)
                    }
                },
            },
        };

        println!("raver");

        let body = serde_json::to_string(&body).unwrap();

        println!("past");

        let path = format!(
            "https://api.sandbox.push.apple.com:443/3/device/{:?}",
            token
        );

        println!("Apple token? {:?}", token);

        let client = reqwest::Client::new();
        let res = client
            .post(path)
            .version(reqwest::Version::HTTP_2)
            .headers(headers)
            .body(reqwest::Body::from(body))
            .send()
            .await
            .map_err(|_e| {
                println!("{}", _e);
                tide::http::Error::from_str(StatusCode::BadRequest, Error::InavlidAuthHeaderError)
            })?;

        println!("rams");

        println!("Notification server is running... {:?}", &res);
    }
    Ok(format!("Testing ",).into())
}

use crate::{
    controllers::decode_token, error::Error, Alert, ClaimsToken, DoorState, NotificationMessage,
    TideState,
};

#[cfg(test)]
mod tests {
    #[async_std::test]
    async fn notification_handler() {
        let email = "Nathaniel".to_string();
        let action = crate::DoorState::Open;

        let mut key_file = "AuthKey_6DRGKC394S.p8".to_string();
        let mut team_id = "Z589Y98374".to_string();
        let mut key_id = "6DRGKC394S".to_string();
        let mut device_token =
            "a7036bba641bedc7841ac40be85612eb32c4cb69ee77ef5f90c9521f62131acc".to_string();
        let mut message = match action {
            crate::DoorState::Open => {
                format!("{} {}ed the door", email, action)
            }
            crate::DoorState::Close => {
                format!("{} {}ed the door", email, action)
            }
        };
        let mut sandbox = true;
        let mut topic: Option<String> = Some("com.nathanielbajo.loginForm".to_string());
        println!("server");

        {
            let mut ap = argparse::ArgumentParser::new();
            ap.set_description("APNs token-based push");
            ap.refer(&mut key_file).add_option(
                &["-p", "--pkcs8"],
                argparse::Store,
                "Private key PKCS8",
            );
            ap.refer(&mut team_id).add_option(
                &["-t", "--team_id"],
                argparse::Store,
                "APNs team ID",
            );
            ap.refer(&mut key_id)
                .add_option(&["-k", "--key_id"], argparse::Store, "APNs key ID");
            ap.refer(&mut device_token).add_option(
                &["-d", "--device_token"],
                argparse::Store,
                "APNs device token",
            );
            ap.refer(&mut message).add_option(
                &["-m", "--message"],
                argparse::Store,
                "Notification message",
            );
            ap.refer(&mut sandbox).add_option(
                &["-s", "--sandbox"],
                argparse::StoreTrue,
                "Use the development APNs servers",
            );
            ap.refer(&mut topic).add_option(
                &["-o", "--topic"],
                argparse::StoreOption,
                "APNS topic",
            );
            ap.parse_args_or_exit();
        }

        println!("server2");

        let mut private_key = std::fs::File::open(key_file).unwrap();

        let endpoint = if sandbox {
            a2::Endpoint::Sandbox
        } else {
            a2::Endpoint::Production
        };

        println!("server3");

        let client = a2::Client::token(&mut private_key, key_id, team_id, endpoint).unwrap();

        let options = a2::NotificationOptions {
            apns_topic: topic.as_ref().map(|s| &**s),
            ..Default::default()
        };

        println!("server4");

        let mut builder = a2::PlainNotificationBuilder::new(message.as_ref());
        builder.set_sound("default");
        builder.set_badge(1u32);

        println!("server5");

        let payload = a2::NotificationBuilder::build(builder, device_token.as_ref(), options);
        println!("server6");

        let response = client.send(payload).await.unwrap();
        println!("server7 {response:?}");
    }
}
