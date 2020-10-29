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
use std::io;
use std::str::FromStr;

use lnpbp::lnp::application::{ChannelId, TempChannelId};
use lnpbp::lnp::NodeEndpoint;
use lnpbp::strict_encoding::{self, StrictDecode, StrictEncode};
use lnpbp_services::esb::ServiceAddress;

/// Identifiers of daemons participating in LNP Node
#[derive(Clone, PartialEq, Eq, Hash, Debug, Display, From)]
pub enum DaemonId {
    #[display("loopback")]
    Loopback,

    #[display("lnpd")]
    Lnpd,

    #[display("gossipd")]
    Gossip,

    #[display("routed")]
    Routing,

    #[display("connectiond<{_0}>")]
    Connection(String),

    #[display("channel<{_0:#x}>")]
    #[from]
    #[from(TempChannelId)]
    Channel(ChannelId),

    #[display("external<{_0}>")]
    Foreign(String),
}

impl DaemonId {
    pub fn router() -> DaemonId {
        DaemonId::Lnpd
    }
}

impl ServiceAddress for DaemonId {}

impl AsRef<[u8]> for DaemonId {
    fn as_ref(&self) -> &[u8] {
        match self {
            DaemonId::Loopback => "loopback".as_bytes(),
            DaemonId::Lnpd => "lnpd".as_bytes(),
            DaemonId::Gossip => "gossipd".as_bytes(),
            DaemonId::Routing => "routed".as_bytes(),
            DaemonId::Connection(endpoint) => endpoint.as_ref(),
            DaemonId::Channel(channel_id) => channel_id.as_inner().as_ref(),
            DaemonId::Foreign(name) => name.as_bytes(),
        }
    }
}

impl From<DaemonId> for Vec<u8> {
    fn from(daemon_id: DaemonId) -> Self {
        daemon_id.as_ref().to_vec()
    }
}

impl From<Vec<u8>> for DaemonId {
    fn from(vec: Vec<u8>) -> Self {
        match vec.as_slice() {
            v if v == "loopback".as_bytes() => DaemonId::Loopback,
            v if v == "lnpd".as_bytes() => DaemonId::Lnpd,
            v if v == "gossipd".as_bytes() => DaemonId::Gossip,
            v if v == "routed".as_bytes() => DaemonId::Routing,
            v => {
                let s = String::from_utf8_lossy(v).to_string();
                if NodeEndpoint::from_str(&s).is_ok() {
                    DaemonId::Connection(s)
                } else if v.len() == 32 {
                    let mut hash = [0u8; 32];
                    hash.copy_from_slice(v);
                    DaemonId::Channel(ChannelId::from_inner(hash.into()))
                } else {
                    DaemonId::Foreign(s)
                }
            }
        }
    }
}

impl StrictEncode for DaemonId {
    type Error = strict_encoding::Error;

    fn strict_encode<E: io::Write>(
        &self,
        mut e: E,
    ) -> Result<usize, Self::Error> {
        Ok(match self {
            DaemonId::Loopback => 0u8.strict_encode(e)?,
            DaemonId::Lnpd => 1u8.strict_encode(e)?,
            DaemonId::Gossip => 2u8.strict_encode(e)?,
            DaemonId::Routing => 3u8.strict_encode(e)?,
            DaemonId::Connection(peer_id) => {
                strict_encode_list!(e; 4u8, peer_id)
            }
            DaemonId::Channel(channel_id) => {
                strict_encode_list!(e; 5u8, channel_id)
            }
            DaemonId::Foreign(id) => strict_encode_list!(e; 6u8, id),
        })
    }
}

impl StrictDecode for DaemonId {
    type Error = strict_encoding::Error;

    fn strict_decode<D: io::Read>(mut d: D) -> Result<Self, Self::Error> {
        let ty = u8::strict_decode(&mut d)?;
        Ok(match ty {
            0 => DaemonId::Loopback,
            1 => DaemonId::Lnpd,
            2 => DaemonId::Gossip,
            3 => DaemonId::Routing,
            4 => DaemonId::Connection(StrictDecode::strict_decode(&mut d)?),
            5 => DaemonId::Channel(StrictDecode::strict_decode(&mut d)?),
            6 => DaemonId::Foreign(StrictDecode::strict_decode(&mut d)?),
            _ => Err(strict_encoding::Error::EnumValueNotKnown(
                s!("DaemonId"),
                ty,
            ))?,
        })
    }
}
