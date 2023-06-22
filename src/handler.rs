use std::time::{Duration, Instant};
use std::collections::HashSet;
use std::ops::Deref;
use actix_ws::Message;
use futures_util::{
    future::{self, Either},
    StreamExt as _,
};
use tokio::{pin, time::interval};

use lib_pipes::{
    tmq_subscribe_connect,
    PORT_P61,
    DECODE_F64,
    DECODE_I64,
    DECODE_BOOL,
    DECODE_DXPRICETYPE,
    DECODE_CHARCODE,
    DECODE_DXTICKDIRECTION,
    DECODE_MIDSTRING,
    DECODE_DXSHORTSALESTATUS,
    DECODE_DXTRADINGSTATUS
};
use lib_msg_dx::{
    DxPriceType,
    CharCode,
    DxTickDirection,
    DxShortSaleStatus,
    DxTradingStatus
};
// use lib_msg_grid::{
//     utils::from_mid_string,
//     types::MidString
// };

/// How often heartbeat pings are sent.
///
/// Should be half (or less) of the acceptable client timeout.
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

/// How long before lack of client response causes a timeout.
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

/// Echo text & binary messages received from the client, respond to ping messages, and monitor
/// connection health to detect network issues and free up resources.
pub async fn echo_heartbeat_ws(
    mut session: actix_ws::Session,
    mut msg_stream: actix_ws::MessageStream,
) {
    log::info!("connected");

    let mut sub_map:HashSet<Vec<u8>> = HashSet::new();

    let mut last_heartbeat = Instant::now();
    let mut interval = interval(HEARTBEAT_INTERVAL);

    let subscriber = tmq_subscribe_connect(PORT_P61).unwrap();
    let mut subscriber = subscriber.subscribe(b"XXXXXXXXXXXXXXXXXXXXXXXXXX").unwrap();

    let reason = loop {
        // create "next client timeout check" future
        let tick = interval.tick();
        // required for select()
        pin!(tick);

        let msg_rx = subscriber.next();
        pin!(msg_rx);

        // TODO: nested select is pretty gross for readability on the match
        let messages = future::select(msg_stream.next(), msg_rx);
        pin!(messages);

        match future::select(messages, tick).await {
            // commands & messages received from client
            Either::Left((Either::Left((Some(Ok(msg)), _)), _)) => {
                log::debug!("msg: {msg:?}");

                match msg {
                    Message::Ping(bytes) => {
                        last_heartbeat = Instant::now();
                        // unwrap:
                        session.pong(&bytes).await.unwrap();
                    }

                    Message::Pong(_) => {
                        last_heartbeat = Instant::now();
                    }

                    Message::Text(text) => {
                        if text == "OFF" {
                            // log::info!("OFF SENT");
                            for sub in &sub_map {
                                subscriber.unsubscribe(sub).unwrap();
                            }
                            sub_map.clear();
                        } else {
                            let text = text.as_bytes();
                            sub_map.insert(text.to_vec());
                            subscriber.subscribe(text).unwrap();
                        }

                        // process_text_msg(&chat_server, &mut session, &text, conn_id, &mut name)
                        //     .await;
                    }

                    Message::Binary(_bin) => {
                        log::warn!("unexpected binary message");
                    }

                    Message::Close(reason) => break reason,

                    _ => {
                        break None;
                    }
                }
            }

            // client WebSocket stream error
            Either::Left((Either::Left((Some(Err(err)), _)), _)) => {
                log::error!("{}", err);
                break None;
            }

            // client WebSocket stream ended
            Either::Left((Either::Left((None, _)), _)) => break None,

            // chat messages received from other room participants
            Either::Left((Either::Right((Some(msg), _)), _)) => {
                let msg = msg.unwrap();
                let msg = msg.iter().map(|d| d.deref()).collect::<Vec<&[u8]>>();
                // let topic = &msg[0];
                // let data = msg[1].deref();

                // for message in msg {
                //     data.extend_from_slice(message.deref());
                // }
                // let s = std::str::from_utf8(msg[0]);
                if sub_map.contains(msg[0]) {
                    let mut data = Vec::new();
                    data.extend_from_slice(b"[\"");
                    data.extend_from_slice(msg[0]);
                    data.extend_from_slice(b"\",");
                    match msg[1] {
                        DECODE_F64 => {
                            data.extend_from_slice(&format!("{}",bincode::deserialize::<f64>(&msg[2]).unwrap()).as_bytes());
                        }
                        DECODE_I64 => {
                            data.extend_from_slice(&format!("{}",bincode::deserialize::<i64>(&msg[2]).unwrap()).as_bytes());
                        }
                        DECODE_BOOL => {
                            data.extend_from_slice(&format!("{}",bincode::deserialize::<bool>(&msg[2]).unwrap()).as_bytes());
                        }
                        DECODE_DXPRICETYPE => {
                            data.extend_from_slice(&format!("'{}'",bincode::deserialize::<DxPriceType>(&msg[2]).unwrap().as_str()).as_bytes());
                        }
                        DECODE_CHARCODE => {
                            // data.extend_from_slice(&bincode::deserialize::<CharCode>(&msg[2]).unwrap().as_u8());
                            data.extend_from_slice(&[bincode::deserialize::<CharCode>(&msg[2]).unwrap().as_u8()]);
                        }
                        DECODE_DXTICKDIRECTION => {
                            data.extend_from_slice(&format!("'{}'",bincode::deserialize::<DxTickDirection>(&msg[2]).unwrap().as_str()).as_bytes());
                        }
                        DECODE_MIDSTRING => {
                            data.extend_from_slice(&format!("'{}'",std::str::from_utf8(bincode::deserialize::<&[u8]>(&msg[2]).unwrap()).unwrap()).as_bytes());
                        }
                        DECODE_DXSHORTSALESTATUS => {
                            data.extend_from_slice(&format!("'{}'",bincode::deserialize::<DxShortSaleStatus>(&msg[2]).unwrap().as_str()).as_bytes());
                        }
                        DECODE_DXTRADINGSTATUS => {
                            data.extend_from_slice(&format!("'{}'",bincode::deserialize::<DxTradingStatus>(&msg[2]).unwrap().as_str()).as_bytes());
                        }
                        _ => {
                            // log::info!("{:?} {:?}",s,t);
                            // log::info!(" {:?} {:?}   {:?}",s,t,msg[2]);
                        }
                    }
                    data.extend_from_slice(b"]");
                    session.binary(data).await.unwrap();
                }

                // data.extend_from_slice(msg[0]);/
                // let slice: &[u8] = data.as_slice();
                // let data = bincode::serialize(&(420.69069 as f32)).unwrap();
                // session.binary(data).await.unwrap();
            }

            // all connection's message senders were dropped
            Either::Left((Either::Right((None, _)), _)) => unreachable!(
                "all connection message senders were dropped; chat server may have panicked"
            ),

            // heartbeat internal tick
            Either::Right((_inst, _)) => {
                // if no heartbeat ping/pong received recently, close the connection
                if Instant::now().duration_since(last_heartbeat) > CLIENT_TIMEOUT {
                    log::info!(
                        "client has not sent heartbeat in over {CLIENT_TIMEOUT:?}; disconnecting"
                    );
                    break None;
                }

                // send heartbeat ping
                let _ = session.ping(b"").await;
            }
        }
    };

    // attempt to close connection gracefully
    let _ = session.close(reason).await;

    log::info!("disconnected");
}
