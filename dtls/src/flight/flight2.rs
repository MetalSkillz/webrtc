use super::*;
use crate::content::*;
use crate::errors::*;
use crate::handshake::handshake_header::*;
use crate::handshake::handshake_message_hello_verify_request::*;
use crate::handshake::*;
use crate::record_layer::record_layer_header::*;

use crate::flight::flight0::flight0parse;
use util::Error;

pub(crate) async fn flight2parse<C: FlightConn>(
    /*context.Context,*/
    c: C,
    state: &mut State,
    cache: &HandshakeCache,
    cfg: &HandshakeConfig,
) -> Result<Flight, (Option<Alert>, Option<Error>)> {
    let (seq, msgs) = match cache
        .full_pull_map(
            state.handshake_recv_sequence,
            &[HandshakeCachePullRule {
                typ: HandshakeType::ClientHello,
                epoch: cfg.initial_epoch,
                is_client: true,
                optional: false,
            }],
        )
        .await
    {
        // No valid message received. Keep reading
        Ok((seq, msgs)) => (seq, msgs),

        // Client may retransmit the first ClientHello when HelloVerifyRequest is dropped.
        // Parse as flight 0 in this case.
        Err(_) => return flight0parse(c, state, cache, cfg).await,
    };

    state.handshake_recv_sequence = seq;

    if let Some(message) = msgs.get(&HandshakeType::ClientHello) {
        // Validate type
        let client_hello = match message {
            HandshakeMessage::ClientHello(client_hello) => client_hello,
            _ => {
                return Err((
                    Some(Alert {
                        alert_level: AlertLevel::Fatal,
                        alert_description: AlertDescription::InternalError,
                    }),
                    None,
                ))
            }
        };

        if client_hello.version != PROTOCOL_VERSION1_2 {
            return Err((
                Some(Alert {
                    alert_level: AlertLevel::Fatal,
                    alert_description: AlertDescription::ProtocolVersion,
                }),
                Some(ERR_UNSUPPORTED_PROTOCOL_VERSION.clone()),
            ));
        }

        if client_hello.cookie.is_empty() {
            return Err((None, None));
        }

        if state.cookie != client_hello.cookie {
            return Err((
                Some(Alert {
                    alert_level: AlertLevel::Fatal,
                    alert_description: AlertDescription::AccessDenied,
                }),
                Some(ERR_COOKIE_MISMATCH.clone()),
            ));
        }

        Ok(Flight::Flight4)
    } else {
        Err((
            Some(Alert {
                alert_level: AlertLevel::Fatal,
                alert_description: AlertDescription::InternalError,
            }),
            None,
        ))
    }
}

pub(crate) async fn flight2generate<C: FlightConn>(
    _c: C,
    state: &mut State,
    _cache: &HandshakeCache,
    _cfg: &HandshakeConfig,
) -> Result<Vec<Packet>, (Option<Alert>, Option<Error>)> {
    state.handshake_send_sequence = 0;
    Ok(vec![Packet {
        record: RecordLayer {
            record_layer_header: RecordLayerHeader {
                protocol_version: PROTOCOL_VERSION1_2,
                ..Default::default()
            },
            content: Content::Handshake(Handshake {
                handshake_header: HandshakeHeader::default(),
                handshake_message: HandshakeMessage::HelloVerifyRequest(
                    HandshakeMessageHelloVerifyRequest {
                        version: PROTOCOL_VERSION1_2,
                        cookie: state.cookie.clone(),
                    },
                ),
            }),
        },
        should_encrypt: false,
        reset_local_sequence_number: false,
    }])
}