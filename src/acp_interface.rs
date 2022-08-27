/*
 * Copyright [2022] <mcr@sandelman.ca>

   Licensed under the Apache License, Version 2.0 (the "License");
   you may not use this file except in compliance with the License.
   You may obtain a copy of the License at

       http://www.apache.org/licenses/LICENSE-2.0

   Unless required by applicable law or agreed to in writing, software
   distributed under the License is distributed on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
   See the License for the specific language governing permissions and
   limitations under the License.
 *
 */

// this module listens for interfaces and then sorts them into three lists
// according to the provided list of acp interfaces, downlink interfaces, and
// interfacs to ignore.
// The lists may include glob(1) wildcards.
//
// Interfaces which match none of the lists are placed into the downlink interface
// list if the list is empty, otherwise, they are ignored
//

//use std::net::Ipv6Addr;

extern crate moz_cbor as cbor;

use async_trait::async_trait;
use netlink_packet_route::link::nlas::State;
use std::net::Ipv6Addr;
use tokio::net::UdpSocket;
//use std::io::Error;
use std::io::ErrorKind;
use std::net::{SocketAddrV6};
//use std::net::{SocketAddr};
use std::sync::Arc;
use futures::lock::Mutex;
use tokio::process::{Command};

use cbor::decoder::decode as cbor_decode;

//use crate::debugoptions::DebugOptions;
//use crate::args::RoosterOptions;
use crate::interface::Interface;
use crate::interface::InterfaceType;
use crate::interface::InterfaceDaemon;
use crate::interface::IfIndex;
use crate::grasp;
use crate::grasp::GraspMessage;

struct AcpInterface {
    pub sock: UdpSocket
}

impl AcpInterface {
    pub fn default(sock: UdpSocket) -> AcpInterface {
        AcpInterface {
            sock: sock
        }
    }

    pub async fn open_grasp_port(ifindex: IfIndex) -> Result<AcpInterface, std::io::Error> {
        use socket2::{Socket, Domain, Type};

        let rsin6 = SocketAddrV6::new(Ipv6Addr::UNSPECIFIED,
                                      grasp::GRASP_PORT as u16, 0, ifindex);

        // create a UDP socket
        let rawfd = Socket::new(Domain::ipv6(), Type::dgram(), None).unwrap();

        // set port/address reuse options.
        rawfd.set_reuse_port(true).unwrap();
        rawfd.set_reuse_address(true).unwrap();
        rawfd.set_nonblocking(true).unwrap();
        match rawfd.bind(&socket2::SockAddr::from(rsin6)) {
            Ok(()) => {
                let udp1 = rawfd.into_udp_socket();
                let recv = UdpSocket::from_std(udp1).unwrap();

                // join it to a multicast group
                let grasp_mcast = "FF02:0:0:0:0:0:0:13".parse::<Ipv6Addr>().unwrap();
                recv.join_multicast_v6(&grasp_mcast, ifindex).unwrap();

                let ssin6 = SocketAddrV6::new(Ipv6Addr::UNSPECIFIED,
                                              0 as u16, 0, ifindex);

                let sock = UdpSocket::bind(ssin6).await.unwrap();
                return Ok(AcpInterface::default(sock));
            },
            Err(err) => {
                if err.kind() == ErrorKind::AddrInUse {
                    println!("Address already in use?");
                }
                Command::new("ss")
                    .arg("-uan")
                    .status().await
                    .expect("ss command failed to start");
                Command::new("ip")
                    .arg("link")
                    .arg("ls")
                    .status().await
                    .expect("ss command failed to start");
                Command::new("ip")
                    .arg("addr")
                    .arg("ls")
                    .status().await
                    .expect("ss command failed to start");
                return Err(err);
            }
        }
    }
}

pub struct AcpInterfaceDaemon {
    interface: Option<Arc<Mutex<AcpInterface>>>
}

impl AcpInterfaceDaemon {
    pub fn default() -> AcpInterfaceDaemon {
        AcpInterfaceDaemon {
            interface: None
        }
    }
}

#[async_trait]
impl InterfaceDaemon for AcpInterfaceDaemon {
    async fn start_daemon(self: &mut Self, ifn: &Interface) -> Result<(), rtnetlink::Error> {

        let ai = AcpInterface::open_grasp_port(ifn.ifindex).await.unwrap();

        let ail = Arc::new(Mutex::new(ai));
        self.interface = Some(ail.clone());

        // ail gets moved into the async loop

        tokio::spawn(async move {
            let mut cnt: u32 = 0;

            loop {
                let mut bufbytes = [0u8; 2048];

                //if debug_graspdaemon {
                //}

                // lock it, read from it and return result
                let results = {
                    let ai = ail.lock().await;
                    println!("listening on GRASP socket {:?}", ai.sock);
                    ai.sock.recv_from(&mut bufbytes).await
                };

                match results {
                    Ok((size, addr)) => {
                        println!("{}: grasp daemon read: {} bytes from {}", cnt, size, addr);
                        let graspmessage = match cbor_decode(&bufbytes) {
                            Ok(cbor) => {
                                match GraspMessage::decode_grasp_message(cbor) {
                                    Ok(msg) => msg,
                                    err @ _ => {
                                        println!("   invalid grasp message: {:?}", err);
                                        continue;
                                    }
                                }
                            },
                            err @ _ => {
                                println!("   invalid cbor in message: {:?}", err);
                                continue;
                            }
                        };

                        // now we have a graspmessage which we'll do something with!
                        println!("{} grasp message: {:?}", cnt, graspmessage);

                        /* insert into list of possible registrars */
                        // todo
                    }
                    Err(msg) => {
                        println!("{} grasp read got error: {:?}", cnt, msg);
                        // deal with socket closed?
                    }
                }

                cnt += 1;
            }

        });

        Ok(())
    }

}


#[cfg(test)]
pub mod tests {
    use super::*;

    #[allow(unused_macros)]
    macro_rules! aw {
        ($e:expr) => {
            tokio_test::block_on($e)
        };
    }

    fn setup_ifn(aifn: AcpInterfaceDaemon) -> Interface {
        let mut ifn = Interface::default();
        ifn.ifindex= 1; // usually lo.
        ifn.ifname = "lo".to_string();
        ifn.ignored= false;
        ifn.mtu    = 1500;
        ifn.oper_state = State::Up;
        ifn.daemon = InterfaceType::AcpUpLink;
        ifn
    }

    async fn async_start_acp(aifn: AcpInterfaceDaemon) -> Result<(), std::io::Error> {
        let mut ifn = setup_ifn(aifn);
        ifn.start_daemon().await;
        Ok(())
    }

    #[test]
    fn test_start_acp() -> Result<(), std::io::Error> {
        //let (_awriter, mut all1) = setup_ai();
        let aifn = AcpInterfaceDaemon::default();
        aw!(async_start_acp(aifn)).unwrap();
        Ok(())
    }

    async fn async_open_socket() -> Result<(), std::io::Error> {
        //let ifn = setup_ifn(None);
        // ifindex=1, is lo
        let _aifn = AcpInterface::open_grasp_port(1).await.unwrap();
        Ok(())
    }

    #[test]
    fn test_open_socket() -> Result<(), std::io::Error> {
        //let (_awriter, mut all1) = setup_ai();
        aw!(async_open_socket()).unwrap();
        Ok(())
    }



}

/*
 * Local Variables:
 * mode: rust
 * compile-command: "cd .. && RUSTFLAGS='-A dead_code -Awarnings' cargo build"
 * End:
 */
