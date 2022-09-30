// use futures::StreamExt;
// pub use mediasoup::prelude::*;
// use participant::ParticipantConnection;
// use room::RoomId;
// pub use rooms_registry::RoomsRegistry;
// use serde::Deserialize;
// use std::num::{NonZeroU32, NonZeroU8};
// use tide::{Request, Response, StatusCode};
// use tide_websockets::{Message, WebSocketConnection};
// use tokio::sync::mpsc;

// use crate::TideState;

// use self::participant::{AllMessages, ClientMessage};

// mod room {
//     use super::participant::ParticipantId;
//     use event_listener_primitives::{Bag, BagOnce, HandlerId};
//     use mediasoup::prelude::*;
//     use mediasoup::worker::{WorkerLogLevel, WorkerLogTag};
//     use parking_lot::Mutex;
//     use serde::{Deserialize, Serialize};
//     use std::collections::HashMap;
//     use std::fmt;
//     use std::sync::{Arc, Weak};
//     use uuid::Uuid;

//     #[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd, Deserialize, Serialize)]
//     pub struct RoomId(Uuid);

//     impl fmt::Display for RoomId {
//         fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//             fmt::Display::fmt(&self.0, f)
//         }
//     }

//     impl RoomId {
//         pub fn new() -> Self {
//             Self(Uuid::new_v4())
//         }
//     }

//     #[derive(Default)]
//     #[allow(clippy::type_complexity)]
//     struct Handlers {
//         producer_add:
//             Bag<Arc<dyn Fn(&ParticipantId, &Producer) + Send + Sync>, ParticipantId, Producer>,
//         producer_remove:
//             Bag<Arc<dyn Fn(&ParticipantId, &ProducerId) + Send + Sync>, ParticipantId, ProducerId>,
//         close: BagOnce<Box<dyn FnOnce() + Send>>,
//     }

//     struct Inner {
//         id: RoomId,
//         router: Router,
//         handlers: Handlers,
//         clients: Mutex<HashMap<ParticipantId, Vec<Producer>>>,
//     }

//     impl fmt::Debug for Inner {
//         fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//             f.debug_struct("Inner")
//                 .field("id", &self.id)
//                 .field("handlers", &"...")
//                 .field("clients", &self.clients)
//                 .finish()
//         }
//     }

//     impl Drop for Inner {
//         fn drop(&mut self) {
//             println!("Room {} closed", self.id);

//             self.handlers.close.call_simple();
//         }
//     }

//     /// Room holds producers of the participants such that other participants can consume audio and
//     /// video tracks of each other
//     #[derive(Debug, Clone)]
//     pub struct Room {
//         inner: Arc<Inner>,
//     }

//     impl Room {
//         /// Create new `Room` with random `RoomId`
//         pub async fn new(worker_manager: &WorkerManager) -> Result<Self, String> {
//             Self::new_with_id(worker_manager, RoomId::new()).await
//         }

//         /// Create new `Room` with a specific `RoomId`
//         pub async fn new_with_id(
//             worker_manager: &WorkerManager,
//             id: RoomId,
//         ) -> Result<Room, String> {
//             let worker = worker_manager
//                 .create_worker({
//                     let mut settings = WorkerSettings::default();
//                     settings.log_level = WorkerLogLevel::Debug;
//                     settings.log_tags = vec![
//                         WorkerLogTag::Info,
//                         WorkerLogTag::Ice,
//                         WorkerLogTag::Dtls,
//                         WorkerLogTag::Rtp,
//                         WorkerLogTag::Srtp,
//                         WorkerLogTag::Rtcp,
//                         WorkerLogTag::Rtx,
//                         WorkerLogTag::Bwe,
//                         WorkerLogTag::Score,
//                         WorkerLogTag::Simulcast,
//                         WorkerLogTag::Svc,
//                         WorkerLogTag::Sctp,
//                         WorkerLogTag::Message,
//                     ];

//                     settings
//                 })
//                 .await
//                 .map_err(|error| format!("Failed to create worker: {}", error))?;
//             let router = worker
//                 .create_router(RouterOptions::new(crate::videoroom::media_codecs()))
//                 .await
//                 .map_err(|error| format!("Failed to create router: {}", error))?;

//             println!("Room {} created", id);

//             Ok(Self {
//                 inner: Arc::new(Inner {
//                     id,
//                     router,
//                     handlers: Handlers::default(),
//                     clients: Mutex::default(),
//                 }),
//             })
//         }

//         /// ID of the room
//         pub fn id(&self) -> RoomId {
//             self.inner.id
//         }

//         /// Get router associated with this room
//         pub fn router(&self) -> &Router {
//             &self.inner.router
//         }

//         /// Add producer to the room, this will trigger notifications to other participants that
//         /// will be able to consume it
//         pub fn add_producer(&self, participant_id: ParticipantId, producer: Producer) {
//             self.inner
//                 .clients
//                 .lock()
//                 .entry(participant_id)
//                 .or_default()
//                 .push(producer.clone());

//             self.inner
//                 .handlers
//                 .producer_add
//                 .call_simple(&participant_id, &producer);
//         }

//         /// Remove participant and all of its associated producers
//         pub fn remove_participant(&self, participant_id: &ParticipantId) {
//             let producers = self.inner.clients.lock().remove(participant_id);

//             for producer in producers.unwrap_or_default() {
//                 let producer_id = &producer.id();
//                 self.inner
//                     .handlers
//                     .producer_remove
//                     .call_simple(participant_id, producer_id);
//             }
//         }

//         /// Get all producers of all participants, useful when new participant connects and needs to
//         /// consume tracks of everyone who is already in the room
//         pub fn get_all_producers(&self) -> Vec<(ParticipantId, ProducerId)> {
//             self.inner
//                 .clients
//                 .lock()
//                 .iter()
//                 .flat_map(|(participant_id, producers)| {
//                     let participant_id = *participant_id;
//                     producers
//                         .iter()
//                         .map(move |producer| (participant_id, producer.id()))
//                 })
//                 .collect()
//         }

//         /// Subscribe to notifications when new producer is added to the room
//         pub fn on_producer_add<F: Fn(&ParticipantId, &Producer) + Send + Sync + 'static>(
//             &self,
//             callback: F,
//         ) -> HandlerId {
//             self.inner.handlers.producer_add.add(Arc::new(callback))
//         }

//         /// Subscribe to notifications when producer is removed from the room
//         pub fn on_producer_remove<F: Fn(&ParticipantId, &ProducerId) + Send + Sync + 'static>(
//             &self,
//             callback: F,
//         ) -> HandlerId {
//             self.inner.handlers.producer_remove.add(Arc::new(callback))
//         }

//         /// Subscribe to notification when room is closed
//         pub fn on_close<F: FnOnce() + Send + 'static>(&self, callback: F) -> HandlerId {
//             self.inner.handlers.close.add(Box::new(callback))
//         }

//         /// Get `WeakRoom` that can later be upgraded to `Room`, but will not prevent room from
//         /// being destroyed
//         pub fn downgrade(&self) -> WeakRoom {
//             WeakRoom {
//                 inner: Arc::downgrade(&self.inner),
//             }
//         }
//     }

//     /// Similar to `Room`, but doesn't prevent room from being destroyed
//     #[derive(Debug, Clone)]
//     pub struct WeakRoom {
//         inner: Weak<Inner>,
//     }

//     impl WeakRoom {
//         /// Upgrade `WeakRoom` to `Room`, may return `None` if underlying room was destroyed already
//         pub fn upgrade(&self) -> Option<Room> {
//             self.inner.upgrade().map(|inner| Room { inner })
//         }
//     }
// }

// mod rooms_registry {
//     use crate::videoroom::room::{Room, RoomId, WeakRoom};
//     use async_lock::Mutex;
//     use mediasoup::prelude::*;
//     use std::collections::hash_map::Entry;
//     use std::collections::HashMap;
//     use std::sync::Arc;

//     #[derive(Debug, Default, Clone)]
//     pub struct RoomsRegistry {
//         // We store `WeakRoom` instead of full `Room` to avoid cycles and to not prevent rooms from
//         // being destroyed when last participant disconnects
//         rooms: Arc<Mutex<HashMap<RoomId, WeakRoom>>>,
//     }

//     impl RoomsRegistry {
//         /// Retrieves existing room or creates a new one with specified `RoomId`
//         pub async fn get_or_create_room(
//             &self,
//             worker_manager: &WorkerManager,
//             room_id: RoomId,
//         ) -> Result<Room, String> {
//             let mut rooms = self.rooms.lock().await;
//             match rooms.entry(room_id) {
//                 Entry::Occupied(mut entry) => match entry.get().upgrade() {
//                     Some(room) => Ok(room),
//                     None => {
//                         println!("Did it reach this side");
//                         let room = Room::new_with_id(worker_manager, room_id).await?;
//                         entry.insert(room.downgrade());
//                         room.on_close({
//                             let room_id = room.id();
//                             let rooms = Arc::clone(&self.rooms);
//                             println!("Omo2");


//                             move || {
//                                 std::thread::spawn(move || {
//                                     futures_lite::future::block_on(async move {
//                                         rooms.lock().await.remove(&room_id);
//                                     });
//                                 });
//                             }
//                         })
//                         .detach();
//                         Ok(room)
//                     }
//                 },
//                 Entry::Vacant(entry) => {
//                     let room = Room::new_with_id(worker_manager, room_id).await?;
//                     entry.insert(room.downgrade());
//                     room.on_close({
//                         let room_id = room.id();
//                         let rooms = Arc::clone(&self.rooms);
//                         println!("Omo3");


//                         move || {
//                             std::thread::spawn(move || {
//                                 futures_lite::future::block_on(async move {
//                                     rooms.lock().await.remove(&room_id);
//                                 });
//                             });
//                         }
//                     })
//                     .detach();
//                     println!("Omo4");

//                     Ok(room)
//                 }
//             }
//         }

//         /// Create new room with random `RoomId`
//         pub async fn create_room(&self, worker_manager: &WorkerManager) -> Result<Room, String> {
//             let mut rooms = self.rooms.lock().await;
//             println!("Roooom {:?}", rooms);
//             let room = Room::new(worker_manager).await?;
//             println!("Room1: {:?}", room);
//             rooms.insert(room.id(), room.downgrade());
//             room.on_close({
//                 let room_id = room.id();
//                 let rooms = Arc::clone(&self.rooms);

//                 move || {
//                     std::thread::spawn(move || {
//                         futures_lite::future::block_on(async move {
//                             rooms.lock().await.remove(&room_id);
//                         });
//                     });
//                 }
//             })
//             .detach();
//             Ok(room)
//         }
//     }
// }

// mod participant {
//     use crate::videoroom::participant::messages::{
//         InternalMessage, ServerMessage, TransportOptions,
//     };
//     use crate::videoroom::room::Room;
//     use event_listener_primitives::HandlerId;
//     use mediasoup::prelude::*;
//     use serde::{Deserialize, Serialize};
//     use std::collections::HashMap;
//     use std::fmt;
//     use std::net::{IpAddr, Ipv4Addr};
//     use tide_websockets::WebSocketConnection;
//     use tokio::sync::mpsc::Sender;
//     use uuid::Uuid;

//     pub use self::messages::{AllMessages, ClientMessage};

//     mod messages {
//         use crate::videoroom::participant::ParticipantId;
//         use crate::videoroom::room::RoomId;
//         use mediasoup::prelude::*;
//         use serde::{Deserialize, Serialize};

//         /// Data structure containing all the necessary information about transport options required
//         /// from the server to establish transport connection on the client
//         #[derive(Serialize, Debug)]
//         #[serde(rename_all = "camelCase")]
//         pub struct TransportOptions {
//             pub id: TransportId,
//             pub dtls_parameters: DtlsParameters,
//             pub ice_candidates: Vec<IceCandidate>,
//             pub ice_parameters: IceParameters,
//         }

//         #[derive(Debug)]
//         pub enum AllMessages {
//             ServerMessage(ServerMessage),
//             InternalMessage(InternalMessage),
//             ClientMessage(ClientMessage),
//         }

//         /// Server messages sent to the client
//         #[derive(Serialize, Debug)]
//         #[serde(tag = "action")]
//         // #[rtype(result = "()")]
//         pub enum ServerMessage {
//             /// Initialization message with consumer/producer transport options and Router's RTP
//             /// capabilities necessary to establish WebRTC transport connection client-side
//             #[serde(rename_all = "camelCase")]
//             Init {
//                 room_id: RoomId,
//                 consumer_transport_options: TransportOptions,
//                 producer_transport_options: TransportOptions,
//                 router_rtp_capabilities: RtpCapabilitiesFinalized,
//             },
//             /// Notification that new producer was added to the room
//             #[serde(rename_all = "camelCase")]
//             ProducerAdded {
//                 participant_id: ParticipantId,
//                 producer_id: ProducerId,
//             },
//             /// Notification that producer was removed from the room
//             #[serde(rename_all = "camelCase")]
//             ProducerRemoved {
//                 participant_id: ParticipantId,
//                 producer_id: ProducerId,
//             },
//             /// Notification that producer transport was connected successfully (in case of error
//             /// connection is just dropped, in real-world application you probably want to handle it
//             /// better)
//             ConnectedProducerTransport,
//             /// Notification that producer was created on the server
//             #[serde(rename_all = "camelCase")]
//             Produced { id: ProducerId },
//             /// Notification that consumer transport was connected successfully (in case of error
//             /// connection is just dropped, in real-world application you probably want to handle it
//             /// better)
//             ConnectedConsumerTransport,
//             /// Notification that consumer was successfully created server-side, client can resume
//             /// the consumer after this
//             #[serde(rename_all = "camelCase")]
//             Consumed {
//                 id: ConsumerId,
//                 producer_id: ProducerId,
//                 kind: MediaKind,
//                 rtp_parameters: RtpParameters,
//             },
//         }

//         /// Client messages sent to the server
//         #[derive(Deserialize, Debug)]
//         #[serde(tag = "action")]
//         // #[rtype(result = "()")]
//         pub enum ClientMessage {
//             /// Client-side initialization with its RTP capabilities, in this simple case we expect
//             /// those to match server Router's RTP capabilities
//             #[serde(rename_all = "camelCase")]
//             Init { rtp_capabilities: RtpCapabilities },
//             /// Request to connect producer transport with client-side DTLS parameters
//             #[serde(rename_all = "camelCase")]
//             ConnectProducerTransport { dtls_parameters: DtlsParameters },
//             /// Request to produce a new audio or video track with specified RTP parameters
//             #[serde(rename_all = "camelCase")]
//             Produce {
//                 kind: MediaKind,
//                 rtp_parameters: RtpParameters,
//             },
//             /// Request to connect consumer transport with client-side DTLS parameters
//             #[serde(rename_all = "camelCase")]
//             ConnectConsumerTransport { dtls_parameters: DtlsParameters },
//             /// Request to consume specified producer
//             #[serde(rename_all = "camelCase")]
//             Consume { producer_id: ProducerId },
//             /// Request to resume consumer that was previously created
//             #[serde(rename_all = "camelCase")]
//             ConsumerResume { id: ConsumerId },
//         }

//         /// Internal actor messages for convenience
//         // #[derive(Message)]
//         // #[rtype(result = "()")]
//         #[derive(Debug)]
//         pub enum InternalMessage {
//             /// Save producer in connection-specific hashmap to prevent it from being destroyed
//             SaveProducer(Producer),
//             /// Save consumer in connection-specific hashmap to prevent it from being destroyed
//             SaveConsumer(Consumer),
//             /// Stop/close the WebSocket connection
//             Stop,
//         }
//     }

//     #[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd, Deserialize, Serialize)]
//     pub struct ParticipantId(Uuid);

//     impl fmt::Display for ParticipantId {
//         fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//             fmt::Display::fmt(&self.0, f)
//         }
//     }

//     impl ParticipantId {
//         fn new() -> Self {
//             Self(Uuid::new_v4())
//         }
//     }

//     /// Consumer/producer transports pair for the client
//     struct Transports {
//         consumer: WebRtcTransport,
//         producer: WebRtcTransport,
//     }

//     /// Actor that will represent WebSocket connection from the client, it will handle inbound and
//     /// outbound WebSocket messages in JSON.
//     ///
//     /// See https://actix.rs/docs/websockets/ for official `actix-web` documentation.
//     pub struct ParticipantConnection {
//         id: ParticipantId,
//         /// RTP capabilities received from the client
//         client_rtp_capabilities: Option<RtpCapabilities>,
//         /// Consumers associated with this client, preventing them from being destroyed
//         consumers: HashMap<ConsumerId, Consumer>,
//         /// Producers associated with this client, preventing them from being destroyed
//         producers: Vec<Producer>,
//         /// Consumer and producer transports associated with this client
//         transports: Transports,
//         /// Room to which the client belongs
//         room: Room,
//         /// Event handlers that were attached and need to be removed when participant connection is
//         /// destroyed
//         attached_handlers: Vec<HandlerId>,
//         sender: Sender<AllMessages>,
//     }

//     impl Drop for ParticipantConnection {
//         fn drop(&mut self) {
//             self.room.remove_participant(&self.id);
//         }
//     }

//     impl ParticipantConnection {
//         /// Create a new instance representing WebSocket connection
//         pub async fn new(room: Room, sender: Sender<AllMessages>) -> Result<Self, String> {
//             // We know that for videoroom example we'll need 2 transports, so we can create both
//             // right away. This may not be the case for real-world applications or you may create
//             // this at a different time and/or in different order.
//             let transport_options =
//                 WebRtcTransportOptions::new(TransportListenIps::new(ListenIp {
//                     ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
//                     announced_ip: None,
//                 }));
//             let producer_transport = room
//                 .router()
//                 .create_webrtc_transport(transport_options.clone())
//                 .await
//                 .map_err(|error| format!("Failed to create producer transport: {}", error))?;

//             let consumer_transport = room
//                 .router()
//                 .create_webrtc_transport(transport_options)
//                 .await
//                 .map_err(|error| format!("Failed to create consumer transport: {}", error))?;

//             Ok(Self {
//                 id: ParticipantId::new(),
//                 client_rtp_capabilities: None,
//                 consumers: HashMap::new(),
//                 producers: vec![],
//                 transports: Transports {
//                     consumer: consumer_transport,
//                     producer: producer_transport,
//                 },
//                 room,
//                 attached_handlers: Vec::new(),
//                 sender,
//             })
//         }
//     }

//     impl ParticipantConnection {
//         pub async fn started(&mut self, ctx: &mut WebSocketConnection) {
//             println!("[participant_id {}] WebSocket connection created", self.id);

//             // We know that both consumer and producer transports will be used, so we sent server
//             // information about both in an initialization message alongside with router
//             // capabilities to the client right after WebSocket connection is established
//             let server_init_message = ServerMessage::Init {
//                 room_id: self.room.id(),
//                 consumer_transport_options: TransportOptions {
//                     id: self.transports.consumer.id(),
//                     dtls_parameters: self.transports.consumer.dtls_parameters(),
//                     ice_candidates: self.transports.consumer.ice_candidates().clone(),
//                     ice_parameters: self.transports.consumer.ice_parameters().clone(),
//                 },
//                 producer_transport_options: TransportOptions {
//                     id: self.transports.producer.id(),
//                     dtls_parameters: self.transports.producer.dtls_parameters(),
//                     ice_candidates: self.transports.producer.ice_candidates().clone(),
//                     ice_parameters: self.transports.producer.ice_parameters().clone(),
//                 },
//                 router_rtp_capabilities: self.room.router().rtp_capabilities().clone(),
//             };
//             let sender = self.sender.clone();

//             self.handle_server_message(server_init_message, ctx).await;

//             // Listen for new producers added to the room
//             self.attached_handlers.push(self.room.on_producer_add({
//                 let own_participant_id = self.id;

//                 move |participant_id, producer| {
//                     if &own_participant_id == participant_id {
//                         return;
//                     }
//                     sender.try_send(AllMessages::ServerMessage(ServerMessage::ProducerAdded {
//                         participant_id: *participant_id,
//                         producer_id: producer.id(),
//                     })).unwrap();
//                 }
//             }));
//             let sender = self.sender.clone();

//             // Listen for producers removed from the the room
//             self.attached_handlers.push(self.room.on_producer_remove({
//                 let own_participant_id = self.id;

//                 move |participant_id, producer_id| {
//                     if &own_participant_id == participant_id {
//                         return;
//                     }
//                     sender.try_send(AllMessages::ServerMessage(ServerMessage::ProducerRemoved {
//                         participant_id: *participant_id,
//                         producer_id: *producer_id,
//                     })).unwrap();
//                 }
//             }));

//             // Notify client about any producers that already exist in the room
//             for (participant_id, producer_id) in self.room.get_all_producers() {
//                 self.sender
//                     .clone()
//                     .send(AllMessages::ServerMessage(ServerMessage::ProducerAdded {
//                         participant_id,
//                         producer_id,
//                     }))
//                     .await.unwrap();
//             }
//         }

//        pub fn stopped(&mut self, _ctx: &mut WebSocketConnection) {
//             println!("[participant_id {}] WebSocket connection closed", self.id);
//         }

//         pub fn handle_client_message(&mut self, message: ClientMessage) {
//             match message {
//                 ClientMessage::Init { rtp_capabilities } => {
//                     // We need to know client's RTP capabilities, those are sent using
//                     // initialization message and are stored in connection struct for future use
//                     self.client_rtp_capabilities.replace(rtp_capabilities);
//                 }
//                 ClientMessage::ConnectProducerTransport { dtls_parameters } => {
//                     let participant_id = self.id;
//                     let transport = self.transports.producer.clone();
//                     // Establish connection for producer transport using DTLS parameters received
//                     // from the client, but doing so in a background task since this handler is
//                     // synchronous

//                     {
//                         let sender = self.sender.clone();
//                         tokio::spawn(async move {
//                             match transport
//                                 .connect(WebRtcTransportRemoteParameters { dtls_parameters })
//                                 .await
//                             {
//                                 Ok(_) => {
//                                     sender
//                                         .send(AllMessages::ServerMessage(
//                                             ServerMessage::ConnectedProducerTransport,
//                                         ))
//                                         .await.unwrap();
//                                     println!(
//                                         "[participant_id {}] Producer transport connected",
//                                         participant_id,
//                                     );
//                                 }
//                                 Err(error) => {
//                                     eprintln!("Failed to connect producer transport: {}", error);
//                                     sender
//                                         .send(AllMessages::InternalMessage(InternalMessage::Stop))
//                                         .await.unwrap();
//                                 }
//                             }
//                         });
//                     }
//                 }
//                 ClientMessage::Produce {
//                     kind,
//                     rtp_parameters,
//                 } => {
//                     let participant_id = self.id;
//                     let transport = self.transports.producer.clone();
//                     let room = self.room.clone();
//                     // Use producer transport to create a new producer on the server with given RTP
//                     // parameters
//                     {
//                         let sender = self.sender.clone();
//                         tokio::spawn(async move {
//                             match transport
//                                 .produce(ProducerOptions::new(kind, rtp_parameters))
//                                 .await
//                             {
//                                 Ok(producer) => {
//                                     let id = producer.id();
//                                     sender
//                                         .send(AllMessages::ServerMessage(ServerMessage::Produced {
//                                             id,
//                                         }))
//                                         .await.unwrap();
//                                     // Add producer to the room so that others can consume it
//                                     room.add_producer(participant_id, producer.clone());
//                                     // Producer is stored in a hashmap since if we don't do it, it will
//                                     // get destroyed as soon as its instance goes out out scope
//                                     sender
//                                         .send(AllMessages::InternalMessage(
//                                             InternalMessage::SaveProducer(producer),
//                                         ))
//                                         .await.unwrap();
//                                     println!(
//                                         "[participant_id {}] {:?} producer created: {}",
//                                         participant_id, kind, id,
//                                     );
//                                 }
//                                 Err(error) => {
//                                     eprintln!(
//                                         "[participant_id {}] Failed to create {:?} producer: {}",
//                                         participant_id, kind, error
//                                     );
//                                     sender
//                                         .send(AllMessages::InternalMessage(InternalMessage::Stop))
//                                         .await.unwrap();
//                                 }
//                             }
//                         });
//                     }
//                 }
//                 ClientMessage::ConnectConsumerTransport { dtls_parameters } => {
//                     let participant_id = self.id;
//                     let transport = self.transports.consumer.clone();
//                     // The same as producer transport, but for consumer transport
//                     {
//                         let sender = self.sender.clone();
//                         tokio::spawn(async move {
//                             match transport
//                                 .connect(WebRtcTransportRemoteParameters { dtls_parameters })
//                                 .await
//                             {
//                                 Ok(_) => {
//                                     sender
//                                         .send(AllMessages::ServerMessage(
//                                             ServerMessage::ConnectedConsumerTransport,
//                                         ))
//                                         .await.unwrap();
//                                     println!(
//                                         "[participant_id {}] Consumer transport connected",
//                                         participant_id,
//                                     );
//                                 }
//                                 Err(error) => {
//                                     eprintln!(
//                                     "[participant_id {}] Failed to connect consumer transport: {}",
//                                     participant_id, error,
//                                 );
//                                     sender
//                                         .send(AllMessages::InternalMessage(InternalMessage::Stop))
//                                         .await.unwrap();
//                                 }
//                             }
//                         });
//                     }
//                 }
//                 ClientMessage::Consume { producer_id } => {
//                     let participant_id = self.id;
//                     let transport = self.transports.consumer.clone();
//                     let rtp_capabilities = match self.client_rtp_capabilities.clone() {
//                         Some(rtp_capabilities) => rtp_capabilities,
//                         None => {
//                             eprintln!(
//                                 "[participant_id {}] Client should send RTP capabilities before \
//                                 consuming",
//                                 participant_id,
//                             );
//                             return;
//                         }
//                     };
//                     // Create consumer for given producer ID, while first making sure that RTP
//                     // capabilities were sent by the client prior to that
//                     {
//                         let sender = self.sender.clone();
//                         tokio::spawn(async move {
//                             let mut options = ConsumerOptions::new(producer_id, rtp_capabilities);
//                             options.paused = true;

//                             match transport.consume(options).await {
//                                 Ok(consumer) => {
//                                     let id = consumer.id();
//                                     let kind = consumer.kind();
//                                     let rtp_parameters = consumer.rtp_parameters().clone();
//                                     sender
//                                         .send(AllMessages::ServerMessage(ServerMessage::Consumed {
//                                             id,
//                                             producer_id,
//                                             kind,
//                                             rtp_parameters,
//                                         }))
//                                         .await.unwrap();
//                                     // Consumer is stored in a hashmap since if we don't do it, it will
//                                     // get destroyed as soon as its instance goes out out scope
//                                     sender
//                                         .send(AllMessages::InternalMessage(
//                                             InternalMessage::SaveConsumer(consumer),
//                                         ))
//                                         .await.unwrap();
//                                     println!(
//                                         "[participant_id {}] {:?} consumer created: {}",
//                                         participant_id, kind, id,
//                                     );
//                                 }
//                                 Err(error) => {
//                                     eprintln!(
//                                         "[participant_id {}] Failed to create consumer: {}",
//                                         participant_id, error,
//                                     );
//                                     sender
//                                         .send(AllMessages::InternalMessage(InternalMessage::Stop))
//                                         .await.unwrap();
//                                 }
//                             }
//                         });
//                     }
//                 }
//                 ClientMessage::ConsumerResume { id } => {
//                     if let Some(consumer) = self.consumers.get(&id).cloned() {
//                         let participant_id = self.id;

//                         tokio::spawn(async move {
//                             match consumer.resume().await {
//                                 Ok(_) => {
//                                     println!(
//                                         "[participant_id {}] Successfully resumed {:?} consumer {}",
//                                         participant_id,
//                                         consumer.kind(),
//                                         consumer.id(),
//                                     );
//                                 }
//                                 Err(error) => {
//                                     println!(
//                                         "[participant_id {}] Failed to resume {:?} consumer {}: {}",
//                                         participant_id,
//                                         consumer.kind(),
//                                         consumer.id(),
//                                         error,
//                                     );
//                                 }
//                             }
//                         });
//                     }
//                 }
//             }
//         }

//         pub async fn handle_server_message(
//             &mut self,
//             message: ServerMessage,
//             ctx: &WebSocketConnection,
//         ) {
//           let tester = ctx.send_string(serde_json::to_string(&message).unwrap())
//                 .await;
//                 println!("tester: {:?}", tester);
//         }

//         pub fn handle_internal_message(&mut self, message: InternalMessage) {
//             match message {
//                 InternalMessage::Stop => {}
//                 InternalMessage::SaveProducer(producer) => {
//                     // Retain producer to prevent it from being destroyed
//                     self.producers.push(producer);
//                 }
//                 InternalMessage::SaveConsumer(consumer) => {
//                     self.consumers.insert(consumer.id(), consumer);
//                 }
//             }
//         }
//     }
// }

// /// List of codecs that SFU will accept from clients
// pub fn media_codecs() -> Vec<RtpCodecCapability> {
//     vec![
//         RtpCodecCapability::Audio {
//             mime_type: MimeTypeAudio::Opus,
//             preferred_payload_type: None,
//             clock_rate: NonZeroU32::new(48000).unwrap(),
//             channels: NonZeroU8::new(2).unwrap(),
//             parameters: RtpCodecParametersParameters::from([("useinbandfec", 1_u32.into())]),
//             rtcp_feedback: vec![RtcpFeedback::TransportCc],
//         },
//         RtpCodecCapability::Video {
//             mime_type: MimeTypeVideo::Vp8,
//             preferred_payload_type: None,
//             clock_rate: NonZeroU32::new(90000).unwrap(),
//             parameters: RtpCodecParametersParameters::default(),
//             rtcp_feedback: vec![
//                 RtcpFeedback::Nack,
//                 RtcpFeedback::NackPli,
//                 RtcpFeedback::CcmFir,
//                 RtcpFeedback::GoogRemb,
//                 RtcpFeedback::TransportCc,
//             ],
//         },
//     ]
// }

// #[derive(Debug, Deserialize, Default)]
// #[serde(rename_all = "camelCase")]
// pub struct QueryParameters {
//     room_id: Option<RoomId>,
// }

// /// Function that receives HTTP request on WebSocket route and upgrades it to WebSocket connection.
// ///
// /// See https://actix.rs/docs/websockets/ for official `actix-web` documentation.
// pub async fn ws_index(
//     request: Request<std::sync::Arc<TideState>>,
//     mut wsc: WebSocketConnection,
// ) -> tide::Result {
//     let rooms_registry = &request.state().room_registry;
//     let worker_manager = &request.state().worker_manager;
//     let query_parameters: QueryParameters = request.query()?;

//     println!("Query: {:?}", query_parameters);

//     let (sender, mut receiver) = mpsc::channel(64);

//     println!("Testing");

//     let room = match query_parameters.room_id {
//         Some(room_id) => {
//             rooms_registry
//                 .get_or_create_room(&worker_manager, room_id)
//                 .await
//         }
//         None => rooms_registry.create_room(&worker_manager).await,
//     };

//     println!("Testing2");

//     println!("Roooom {:?}", room);

//     let room = match room {
//         Ok(room) => room,
//         Err(error) => {
//             println!("{}", error);
//             let res = Response::new(StatusCode::NotFound);
//             return Ok(res);
//         }
//     };

//     println!("Room: {:?}", &room);

//     let mut participant = ParticipantConnection::new(room, sender.clone())
//         .await
//         .map_err(|_e| {
//             println!("{}", _e);
//             tide::http::Error::from_str(
//                 StatusCode::BadRequest,
//                 crate::error::Error::InavlidAuthHeaderError,
//             )
//         })?;

//     participant.started(&mut wsc).await;
//     participant.stopped(&mut wsc);
//     let mut wsc_clone = wsc.clone();

//     tokio::spawn(async move {
//         while let Some(Ok(Message::Text(message))) = wsc_clone.next().await {
//             println!("Message {message}");
//             match serde_json::from_str::<ClientMessage>(&message) {
//                 Ok(message) => {
//                     // Parse JSON into an enum and just send it back to the actor to be
//                     // processed by another handler below, it is much more convenient to just
//                     // parse it in one place and have typed data structure everywhere else
//                     let res = sender.send(AllMessages::ClientMessage(message)).await;
//                     println!("res: {:?}", &res);
//                 }
//                 Err(error) => {
//                     eprintln!("Failed to parse client message: {:?}", error);
//                 }
//             }
//         }
//     });

//     while let Some(msg) = receiver.recv().await {
//         match msg {
//             AllMessages::ServerMessage(srv_msg) => {
//                 participant.handle_server_message(srv_msg, &wsc).await
//             }
//             AllMessages::InternalMessage(int_msg) => participant.handle_internal_message(int_msg),
//             AllMessages::ClientMessage(client_message) => {
//                 participant.handle_client_message(client_message)
//             }
//         };
//     }
//     Ok(format!("Action executed").into())
// }

// // #[actix_web::main]
// // async fn main() -> std::io::Result<()> {
// //     env_logger::init();

// //     // We will reuse the same worker manager across all connections, this is more than enough for
// //     // this use case
// //     let worker_manager = Data::new(WorkerManager::new());
// //     // Rooms registry will hold all the active rooms
// //     let rooms_registry = Data::new(RoomsRegistry::default());

// //     println!("Server is running... ");
// //     HttpServer::new(move || {
// //         App::new()
// //             .app_data(worker_manager.clone())
// //             .app_data(rooms_registry.clone())
// //             .route("/ws", web::get().to(ws_index))
// //     })
// //     // 2 threads is plenty for this example, default is to have as many threads as CPU cores
// //     .workers(2)
// //     .bind("0.0.0.0:3000")?
// //     .run()
// //     .await
// // }
