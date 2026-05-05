use crate::{
    controllers::decode_token,
    error::Error,
    prisma::{
        Door, DoorUpdateInput, DoorUpdateInputState, DoorWhereUniqueInput, FindFirstUserArgs,
        FindManyUserArgs, UpdateOneDoorArgs, User, UserWhereInput, UserWhereInputEmail,
    },
    DoorState, Polling, TideState,
};
use a2::{Client, Endpoint, NotificationBuilder, NotificationOptions, PlainNotificationBuilder};
use argparse::{ArgumentParser, Store, StoreOption, StoreTrue};
use std::sync::Arc;
use tide::{Error as TideError, Request, StatusCode};

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

    let ringing = req
        .state()
        .ringing
        .load(std::sync::atomic::Ordering::SeqCst);

    println!("door state: {:?}, ringing: {}", door, ringing);

    let mut res = tide::Response::new(StatusCode::Accepted);
    res.set_body(
        tide::Body::from_json(&Polling {
            door: door.state,
            ringing,
        })
        .unwrap(),
    );

    Ok(res)
}

pub async fn toggle_door_state(req: Request<Arc<TideState>>) -> tide::Result {
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

    let door = req
        .state()
        .prisma
        .door::<Door>(DoorWhereUniqueInput { id: Some(1) })
        .await
        .map_err(|_e| {
            tide::http::Error::from_str(StatusCode::BadRequest, Error::InavlidAuthHeaderError)
        })?
        .unwrap();

    let log = format!("{},{},{}", email, chrono::Utc::now(), &door.state);
    let _ = std::fs::write("log.txt", log.as_bytes());
    println!("{}", log);

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

    req.state()
        .ringing
        .store(false, std::sync::atomic::Ordering::SeqCst);

    req.state()
        .prisma
        .update_door::<Door>(UpdateOneDoorArgs {
            data: DoorUpdateInput {
                state: Some(DoorUpdateInputState::String(door_state.to_string())),
            },
            filter: DoorWhereUniqueInput { id: Some(1) },
        })
        .await
        .map_err(|e| TideError::from_str(400, format!("Action invalid: {}", e)))?;

    let message = match door_state {
        DoorState::Open => format!("{} {}ed the door", user.name, door_state),
        DoorState::Close => format!("{} {}d the door", user.name, door_state),
    };

    notification_handler(req.state().clone(), message).await?;

    Ok("Action executed".into())
}

pub async fn notification_handler(state: Arc<TideState>, message: String) -> tide::Result {
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

    for mut device_token in tokens {
        let mut key_file = "*****.p8".to_string();
        let mut team_id = "*****".to_string();
        let mut key_id = "*****".to_string();
        let mut message = message.clone();
        let mut sandbox = true;
        let mut topic: Option<String> = Some("*****".to_string());

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

        let mut private_key = std::fs::File::open(key_file).unwrap();

        let endpoint = if sandbox {
            Endpoint::Sandbox
        } else {
            Endpoint::Production
        };

        let client = Client::token(&mut private_key, key_id, team_id, endpoint).unwrap();

        let options = NotificationOptions {
            apns_topic: topic.as_ref().map(|s| &**s),
            ..Default::default()
        };

        let mut builder = PlainNotificationBuilder::new(message.as_ref());
        builder.set_sound("default");
        builder.set_badge(1u32);

        let payload = builder.build(device_token.as_ref(), options);

        let response = client.send(payload).await.map_err(|e| {
            println!("APNs send error: {}", e);
            tide::http::Error::from_str(StatusCode::BadRequest, Error::InavlidAuthHeaderError)
        })?;

        println!("APNs response: {:?}", response);
    }

    Ok("Notifications dispatched".into())
}
