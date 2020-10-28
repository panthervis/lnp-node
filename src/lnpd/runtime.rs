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

use amplify::Wrapper;
use core::convert::TryInto;
use std::collections::HashMap;
use std::io;
use std::process;

use lnpbp::bitcoin::hashes::hex::ToHex;
use lnpbp::lnp::transport::zmqsocket;
use lnpbp::lnp::{message, ChannelId, Messages, TypedEnum};
use lnpbp_services::esb;
use lnpbp_services::node::TryService;
use lnpbp_services::rpc;

use crate::rpc::{request, Endpoints, Request};
use crate::{Config, DaemonId, Error};

pub fn run(config: Config) -> Result<(), Error> {
    debug!("Staring RPC service runtime");
    let runtime = Runtime {
        opening_channels: none!(),
    };
    let rpc = esb::Controller::init(
        DaemonId::Lnpd,
        map! {
            Endpoints::Msg => rpc::EndpointCarrier::Address(
                config.msg_endpoint.try_into()
                    .expect("Only ZMQ RPC is currently supported")
            ),
            Endpoints::Ctl => rpc::EndpointCarrier::Address(
                config.ctl_endpoint.try_into()
                    .expect("Only ZMQ RPC is currently supported")
            )
        },
        runtime,
        zmqsocket::ApiType::EsbService,
    )?;
    info!("lnpd started");
    rpc.run_or_panic("lnpd");
    unreachable!()
}

pub struct Runtime {
    opening_channels: HashMap<DaemonId, message::OpenChannel>,
}

impl esb::Handler<Endpoints> for Runtime {
    type Request = Request;
    type Address = DaemonId;
    type Error = Error;

    fn handle(
        &mut self,
        senders: &mut esb::Senders<Endpoints>,
        endpoint: Endpoints,
        source: DaemonId,
        request: Request,
    ) -> Result<(), Self::Error> {
        match endpoint {
            Endpoints::Msg => self.handle_rpc_msg(senders, source, request),
            Endpoints::Ctl => self.handle_rpc_ctl(senders, source, request),
            _ => {
                Err(Error::NotSupported(Endpoints::Bridge, request.get_type()))
            }
        }
    }
}

impl Runtime {
    fn handle_rpc_msg(
        &mut self,
        senders: &mut esb::Senders<Endpoints>,
        source: DaemonId,
        request: Request,
    ) -> Result<(), Error> {
        debug!("MSG RPC request from {}: {}", source, request);
        match request {
            Request::LnpwpMessage(Messages::OpenChannel(open_channel)) => {
                info!("Creating channel by peer request from {}", source);
                self.open_channel(senders, source, open_channel)?;
            }

            Request::LnpwpMessage(_) => {
                // Ignore the rest of LN peer messages
            }

            _ => {
                error!(
                    "MSG RPC can be only used for forwarding LNPWP messages"
                );
                return Err(Error::NotSupported(
                    Endpoints::Msg,
                    request.get_type(),
                ));
            }
        }
        Ok(())
    }

    fn handle_rpc_ctl(
        &mut self,
        senders: &mut esb::Senders<Endpoints>,
        source: DaemonId,
        request: Request,
    ) -> Result<(), Error> {
        debug!("CTL RPC request from {}: {}", source, request);
        match request {
            Request::CreateChannel(request::CreateChannel {
                channel_req,
                connectiond,
            }) => {
                info!("Creating channel by request from {}", source);
                self.open_channel(senders, connectiond, channel_req)?;
            }

            Request::Connect => {
                // Ignoring; this is used to set remote identity at ZMQ level

                // Tell channeld channel options and link it with the connection
                // daemon
                debug!("Requesting channeld to open a channel");
                let open_channel = self
                    .opening_channels
                    .get(&source)
                    .ok_or(Error::Other(s!("Unknown channel")))?;
                senders.send_to(
                    Endpoints::Ctl,
                    source.clone(),
                    Request::CreateChannel(request::CreateChannel {
                        channel_req: open_channel.clone(),
                        connectiond: source,
                    }),
                )?;
            }

            _ => {
                error!("Request is not supported by the CTL interface");
                return Err(Error::NotSupported(
                    Endpoints::Ctl,
                    request.get_type(),
                ));
            }
        }

        Ok(())
    }

    fn open_channel(
        &mut self,
        _senders: &mut esb::Senders<Endpoints>,
        _source: DaemonId,
        open_channel: message::OpenChannel,
    ) -> Result<(), Error> {
        debug!("Instantiating channeld...");

        // Start channeld
        launch("channeld", vec![open_channel.temporary_channel_id.to_hex()])
            .and_then(|child| {
                debug!(
                    "New instance of channeld launched with PID {}",
                    child.id()
                );
                Ok(())
            })?;

        self.opening_channels.insert(
            DaemonId::Channel(ChannelId::from_inner(
                open_channel.temporary_channel_id.into_inner(),
            )),
            open_channel,
        );

        Ok(())
    }
}

fn launch(name: &str, args: Vec<String>) -> io::Result<process::Child> {
    let mut bin_path = std::env::current_exe().map_err(|err| {
        error!("Unable to detect binary directory: {}", err);
        err
    })?;
    bin_path.pop();

    let mut orig_args = std::env::args().collect::<Vec<String>>();
    orig_args.extend(args);

    bin_path.push(name);
    #[cfg(target_os = "windows")]
    bin_path.set_extension("exe");

    debug!(
        "Launching {} as a separate process using `{}` as binary",
        name,
        bin_path.to_string_lossy()
    );

    process::Command::new("sh")
        .arg("-C")
        .arg(bin_path)
        .arg("--")
        .args(orig_args)
        .spawn()
        .map_err(|err| {
            error!("Error launching {}: {}", name, err);
            err
        })
}
