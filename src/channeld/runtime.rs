// LNP Node: node running lightning network protocol and generalized lightning
// channels.
// Written in 2020 by
//     Dr. Maxim Orlovsky <orlovsky@pandoracore.com>
//
// To the extent possible under law, the author(s) have dedicated all
// copyright and related and neighboring rights to this software to
// the public domain worldwide. This software is distributed without
// any warranty.
//
// You should have received a copy of the MIT License
// along with this software.
// If not, see <https://opensource.org/licenses/MIT>.

use core::convert::TryInto;
use std::thread::sleep;
use std::time::Duration;

use lnpbp::bitcoin::secp256k1;
use lnpbp::lnp::transport::zmqsocket;
use lnpbp::lnp::{message, ChannelId, Messages, TypedEnum};
use lnpbp_services::esb::{self, Handler};
use lnpbp_services::node::TryService;

use crate::rpc::{request, Request, ServiceBus};
use crate::{Config, DaemonId, Error};

pub fn run(config: Config, channel_id: ChannelId) -> Result<(), Error> {
    debug!("Staring RPC service runtime");
    let runtime = Runtime {
        identity: DaemonId::Channel(channel_id),
    };
    let mut esb = esb::Controller::with(
        map! {
            ServiceBus::Msg => zmqsocket::Carrier::Locator(
                config.msg_endpoint.try_into()
                    .expect("Only ZMQ RPC is currently supported")
            ),
            ServiceBus::Ctl => zmqsocket::Carrier::Locator(
                config.ctl_endpoint.try_into()
                    .expect("Only ZMQ RPC is currently supported")
            )
        },
        DaemonId::router(),
        runtime,
        zmqsocket::ApiType::EsbClient,
    )?;

    // We have to sleep in order for ZMQ to bootstrap
    sleep(Duration::from_secs(1));
    info!("channeld started");
    esb.send_to(ServiceBus::Ctl, DaemonId::Lnpd, Request::Hello)?;
    esb.run_or_panic("channeld");
    unreachable!()
}

pub struct Runtime {
    identity: DaemonId,
}

impl esb::Handler<ServiceBus> for Runtime {
    type Request = Request;
    type Address = DaemonId;
    type Error = Error;

    fn identity(&self) -> DaemonId {
        self.identity.clone()
    }

    fn handle(
        &mut self,
        senders: &mut esb::Senders<ServiceBus, DaemonId>,
        bus: ServiceBus,
        source: DaemonId,
        request: Request,
    ) -> Result<(), Self::Error> {
        match bus {
            ServiceBus::Msg => self.handle_rpc_msg(senders, source, request),
            ServiceBus::Ctl => self.handle_rpc_ctl(senders, source, request),
            _ => {
                Err(Error::NotSupported(ServiceBus::Bridge, request.get_type()))
            }
        }
    }

    fn handle_err(&mut self, _: esb::Error) -> Result<(), esb::Error> {
        // We do nothing and do not propagate error; it's already being reported
        // with `error!` macro by the controller. If we propagate error here
        // this will make whole daemon panic
        Ok(())
    }
}

impl Runtime {
    fn handle_rpc_msg(
        &mut self,
        _senders: &mut esb::Senders<ServiceBus, DaemonId>,
        _source: DaemonId,
        request: Request,
    ) -> Result<(), Error> {
        match request {
            Request::LnpwpMessage(_) => {
                // Ignore the rest of LN peer messages
            }

            _ => {
                error!(
                    "MSG RPC can be only used for forwarding LNPWP messages"
                );
                return Err(Error::NotSupported(
                    ServiceBus::Msg,
                    request.get_type(),
                ));
            }
        }
        Ok(())
    }

    fn handle_rpc_ctl(
        &mut self,
        senders: &mut esb::Senders<ServiceBus, DaemonId>,
        _source: DaemonId,
        request: Request,
    ) -> Result<(), Error> {
        match request {
            Request::CreateChannel(request::CreateChannel {
                channel_req,
                connectiond,
            }) => {
                let dumb_key = secp256k1::PublicKey::from_secret_key(
                    &lnpbp::SECP256K1,
                    &secp256k1::key::ONE_KEY,
                );
                senders.send_to(
                    ServiceBus::Msg,
                    self.identity(),
                    connectiond,
                    Request::LnpwpMessage(Messages::AcceptChannel(
                        message::AcceptChannel {
                            temporary_channel_id: channel_req
                                .temporary_channel_id,
                            dust_limit_satoshis: channel_req
                                .dust_limit_satoshis,
                            max_htlc_value_in_flight_msat: channel_req
                                .max_htlc_value_in_flight_msat,
                            channel_reserve_satoshis: channel_req
                                .channel_reserve_satoshis,
                            htlc_minimum_msat: channel_req.htlc_minimum_msat,
                            minimum_depth: 3, // TODO: take from config options
                            to_self_delay: channel_req.to_self_delay,
                            max_accepted_htlcs: channel_req.max_accepted_htlcs,
                            funding_pubkey: dumb_key,
                            revocation_basepoint: dumb_key,
                            payment_point: dumb_key,
                            delayed_payment_basepoint: dumb_key,
                            htlc_basepoint: dumb_key,
                            first_per_commitment_point: dumb_key,
                            shutdown_scriptpubkey: None,
                            unknown_tlvs: none!(),
                        },
                    )),
                )?;
            }

            _ => {
                error!("Request is not supported by the CTL interface");
                return Err(Error::NotSupported(
                    ServiceBus::Ctl,
                    request.get_type(),
                ));
            }
        }
        Ok(())
    }
}
