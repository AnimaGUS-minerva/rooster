/*
 * Copyright [2022,2023] <mcr@sandelman.ca>

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

// 1. this module announces the join proxy using GRASP messages.
//
// 2. it listens on UDP and TCP socket, and then plugs the connection through.
//

//use std::net::Ipv6Addr;

extern crate moz_cbor as cbor;

use std::net::Ipv6Addr;
//use tokio::net::{UdpSocket, TcpSocket, TcpListener};
use tokio::time::{sleep, Duration};
use tokio::net::{UdpSocket, TcpListener};
//use std::io::Error;
use std::io::ErrorKind;
use std::net::{SocketAddrV6};
//use std::net::{SocketAddr};
use std::sync::Arc;
use futures::lock::Mutex;
//use tokio::process::{Command};
use socket2::{Socket, Domain, Type};

//use cbor::decoder::decode as cbor_decode;

//use crate::debugoptions::DebugOptions;
//use crate::args::RoosterOptions;
use crate::interface::Interface;
use crate::interface::IfIndex;
//use crate::grasp;
//use crate::grasp::GraspMessage;
//use crate::debugoptions::DebugOptions;

pub struct JoinInterface {
    pub grasp_sock: UdpSocket,
    pub stateless_sock: UdpSocket,
    pub stateful_sock: UdpSocket,
    pub https_sock: TcpListener,
}

impl JoinInterface {
    fn open_bound_udpsocket(ifindex: IfIndex, _socknum: u16) -> Result<tokio::net::UdpSocket, std::io::Error> {

        /* this is an announce socket, so let kernel decide on port number */
        let rsin6 = SocketAddrV6::new(Ipv6Addr::UNSPECIFIED,
                                      0, 0, ifindex);

        // create a UDP socket
        let rawfd = Socket::new(Domain::ipv6(), Type::dgram(), None).unwrap();

        // set port/address reuse options.
        rawfd.set_reuse_port(true).unwrap();
        rawfd.set_reuse_address(true).unwrap();
        rawfd.set_nonblocking(true).unwrap();
        match rawfd.bind(&socket2::SockAddr::from(rsin6)) {
            Ok(()) => {
                let udp1 = rawfd.into_udp_socket();
                UdpSocket::from_std(udp1)
            },
            Err(err) => {
                if err.kind() == ErrorKind::AddrInUse {
                    println!("Announce address already in use? {:?}", rsin6);
                }
                return Err(err);
            }
        }
    }

    fn open_bound_tcpsocket(ifindex: IfIndex, socknum: u16) -> Result<tokio::net::TcpListener, std::io::Error> {
        /* let kernel decide on port number */
        let rsin6 = SocketAddrV6::new(Ipv6Addr::UNSPECIFIED,
                                      socknum, 0, ifindex);

        // create a TCP socket
        let rawfd = Socket::new(Domain::ipv6(), Type::stream(), None).unwrap();

        // set port/address reuse options.
        rawfd.set_reuse_port(true).unwrap();
        rawfd.set_reuse_address(true).unwrap();
        rawfd.set_nonblocking(true).unwrap();
        match rawfd.bind(&socket2::SockAddr::from(rsin6)) {
            Ok(()) => {
                let listener = TcpListener::from_std(rawfd.into())?;
                Ok(listener)
            },
            Err(err) => {
                if err.kind() == ErrorKind::AddrInUse {
                    println!("Announce address already in use? {:?}", rsin6);
                }
                return Err(err);
            }
        }
    }

    pub async fn open_ports(ifindex: IfIndex) -> Result<JoinInterface, std::io::Error> {

        let grasp_sock     = JoinInterface::open_bound_udpsocket(ifindex, 0)?;
        let stateless_sock = JoinInterface::open_bound_udpsocket(ifindex, 0)?;
        let stateful_sock  = JoinInterface::open_bound_udpsocket(ifindex, 0)?;
        let https_sock     = JoinInterface::open_bound_tcpsocket(ifindex, 0)?;

        /* now open a UDP socket for plugging through to Registrar */

        return Ok(JoinInterface {
            grasp_sock,stateless_sock,stateful_sock,https_sock
        })
    }

    // make an announcement of that kind of registrar.
    pub async fn registrar_all_announce(self: &JoinInterface) -> Result<(), std::io::Error> {
        Ok(())
    }

    pub async fn start_daemon(ifn: &Interface,
                              _invalidate: Arc<Mutex<bool>>) -> Result<Arc<Mutex<JoinInterface>>, rtnetlink::Error> {

        let ai = JoinInterface::open_ports(ifn.ifindex).await.unwrap();

        let ail = Arc::new(Mutex::new(ai));
        let ai2 = ail.clone();

        // ail gets moved into the async loop

        tokio::spawn(async move {
            let mut cnt: u32 = 0;

            loop {
                //let mut bufbytes = [0u8; 2048];

                //if debug_graspdaemon {
                //}

                println!("{} join loop: ", cnt);
                sleep(Duration::from_millis(6000)).await;

                cnt += 1;
            }

        });

        Ok(ai2)
    }

}


#[cfg(test)]
pub mod tests {
    use super::*;
    use netlink_packet_route::link::nlas::State;
    use crate::interfaces::AllInterfaces;
    use crate::debugoptions::DebugOptions;

    #[allow(unused_macros)]
    macro_rules! aw {
        ($e:expr) => {
            tokio_test::block_on($e)
        };
    }

    fn setup_ai() -> (Arc<Mutex<Vec<u8>>>, AllInterfaces) {
        let writer: Vec<u8> = vec![];
        let awriter = Arc::new(Mutex::new(writer));
        let db1 = DebugOptions { debug_interfaces: true,
                                 verydebug_interfaces: false,
                                 debug_output: awriter.clone() };
        let mut all1 = AllInterfaces::default();
        all1.debug = Arc::new(db1);

        (awriter, all1)
    }

    fn setup_ifn() -> Interface {
        let (_awriter, all1) = setup_ai();
        let mut ifn = Interface::default(all1.debug);
        ifn.ifindex= 1; // usually lo.
        ifn.ifname = "lo".to_string();
        ifn.ignored= false;
        ifn.mtu    = 1500;
        ifn.oper_state = State::Up;
        ifn.acp_daemon = None;
        ifn.join_daemon = None;
        ifn
    }

    fn setup_invalidated_bool() -> Arc<Mutex<bool>> {
        Arc::new(Mutex::new(false))
    }

    async fn async_start_join() -> Result<(), std::io::Error> {
        let     ifn = setup_ifn();
        JoinInterface::start_daemon(&ifn, setup_invalidated_bool()).await.unwrap();
        Ok(())
    }

    #[test]
    fn test_start_join() -> Result<(), std::io::Error> {
        //let (_awriter, mut all1) = setup_ai();
        aw!(async_start_join()).unwrap();
        Ok(())
    }

    async fn async_open_socket() -> Result<(), std::io::Error> {
        //let ifn = setup_ifn(None);
        // ifindex=1, is lo
        let _aifn = JoinInterface::open_ports(1).await.unwrap();
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
 * compile-command: "cd .. && cargo test"
 * End:
 */
