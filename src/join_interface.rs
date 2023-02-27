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

extern crate moz_cbor as cbor;

use tokio::time::{sleep, Duration};
use tokio::net::{UdpSocket, TcpListener, TcpStream};
//use std::io::Error;
use std::io::ErrorKind;
use std::net::{SocketAddrV6};
use std::net::{SocketAddr, IpAddr, Ipv6Addr};
use std::sync::Arc;
use futures::lock::Mutex;
//use tokio::process::{Command};
use socket2::{Socket, Domain, Type};
use netlink_packet_sock_diag::constants::{IPPROTO_TCP, IPPROTO_UDP};

//use cbor::decoder::decode as cbor_decode;

//use crate::debugoptions::DebugOptions;
//use crate::args::RoosterOptions;
use crate::interface::Interface;
use crate::interface::IfIndex;
use crate::interfaces::ProxiesEnabled;
use crate::interfaces::AllInterfaces;
use crate::grasp::{SessionID, GraspMessage, GraspObjective, GraspLocator, GraspMessageType};
use crate::debugoptions::DebugOptions;

pub struct JoinSockets {
    pub stateless_sock: UdpSocket,
    pub stateful_sock: UdpSocket,
    pub https_sock: TcpListener,
}

pub struct JoinInterface {
    pub debug:      Arc<DebugOptions>,
    pub https_v6addr: Ipv6Addr,
    pub https_port:   u16,
    pub grasp_sock: UdpSocket,
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

    fn open_bound_tcpsocket(ifindex: IfIndex, portnum: u16) -> Result<tokio::net::TcpListener, std::io::Error> {
        /* bind this to the IPv6 Link Local address by IFINDEX */
        let rsin6 = SocketAddrV6::new(Ipv6Addr::UNSPECIFIED,
                                      portnum, 0, ifindex);

        // create a TCP socket
        let rawfd = Socket::new(Domain::ipv6(), Type::stream(), None).unwrap();

        // set port/address reuse options.
        rawfd.set_reuse_port(true).unwrap();
        rawfd.set_reuse_address(true).unwrap();
        rawfd.set_nonblocking(true).unwrap();
        let sock6 = &socket2::SockAddr::from(rsin6);
        //println!("tcp socket: index: {} {:?}, {:?}", ifindex, rawfd, sock6);
        match rawfd.bind(sock6) {
            Ok(()) => {
                rawfd.listen(128)?;
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

    pub async fn open_ports(debug: Arc<DebugOptions>,
                            ifindex: IfIndex) -> Result<(JoinInterface,JoinSockets), std::io::Error> {

        let grasp_sock     = JoinInterface::open_bound_udpsocket(ifindex, 0)?;
        let stateless_sock = JoinInterface::open_bound_udpsocket(ifindex, 0)?;
        let stateful_sock  = JoinInterface::open_bound_udpsocket(ifindex, 0)?;
        let https_sock     = JoinInterface::open_bound_tcpsocket(ifindex, 0)?;

        /* now open a UDP socket for plugging through to Registrar */
        let (https_v6addr, https_port) = match https_sock.local_addr() {
            Ok(SocketAddr::V6(v6)) => { (v6.ip().clone(), v6.port()) }
            _ => { return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "must be IPv6".to_string())) },
        };

        return Ok((JoinInterface {
            debug,
            grasp_sock,
            https_v6addr, https_port
        }, JoinSockets {
            stateless_sock,
            stateful_sock,
            https_sock
        }))
    }

    // make an announcement of that kind of registrar.
    pub async fn build_an_proxy(self: &JoinInterface,
                                ifn:  &Interface,
                                proxies: ProxiesEnabled,
                                id: SessionID) -> Result<Vec<u8>, std::io::Error> {

        // all of the sockets are bound to ::, with an ifindex, so to get the
        // right address to announce, need to use the one in the ifn.
        let initiator = ifn.linklocal6;
        let mut gm = GraspMessage {
            mtype: GraspMessageType::M_FLOOD,
            session_id: id,
            initiator: initiator,
            ttl:       1,         // do not leave local network
            objectives: vec![],
        };

        if proxies.http_avail {
            self.debug.debug_verbose(format!("HTTP Registrar at {}:{}",
                                             self.https_v6addr, self.https_port)).await;
            gm.objectives.push(GraspObjective { objective_name: "AN_Proxy".to_string(),
                                                objective_flags: 0,
                                                loop_count: 1,
                                                objective_value: Some("".to_string()),
                                                locator: Some(GraspLocator::O_IPv6_LOCATOR {
                                                    v6addr: initiator,
                                                    transport_proto: IPPROTO_TCP,
                                                    port_number: self.https_port
                                                })});
        }
        if proxies.stateful_avail {
            println!("hello");
        }
        if proxies.stateless_avail {
            println!("stateless");
        }

        // turn it into some bytes.
        let ct = gm.encode_dull_grasp_message().unwrap();
        Ok(ct.serialize())
    }

    // make an announcement of that kind of registrar.
    pub async fn registrar_all_announce(self: &JoinInterface,
                                        ifn:  &Interface,
                                        proxies: ProxiesEnabled,
                                        id: SessionID) -> Result<(), std::io::Error> {

        let mflood_err = self.build_an_proxy(ifn, proxies, id).await;
        let mflood = match mflood_err {
            Ok(x) => { x },
            Err(err) => {
                if err.kind() == ErrorKind::InvalidData {
                    return Ok(());
                } else {
                    return Err(err);
                }
            }
        };

        self.debug.debug_verbose("sending GRASP DULL message".to_string()).await;
        // now write it to socket.
        let graspdest = SocketAddr::new(IpAddr::V6(Ipv6Addr::new(0xff02, 0,
                                                                 0,0,
                                                                 0,0,
                                                                 0,0x13)), 7017);
        self.grasp_sock.send_to(&mflood, graspdest).await.unwrap();

        Ok(())
    }

    pub async fn proxy_https(lji: Arc<Mutex<JoinInterface>>,
                             lallif: Arc<Mutex<AllInterfaces>>,
                             mut socket: tokio::net::TcpStream,
                             pledgeaddr: std::net::SocketAddr) -> Result<(), std::io::Error> {
        let debug = {
            let ji = lji.lock().await;
            ji.debug.clone()
        };

        // need to find a useful Registrar to connect to.
        if let Some(target_sockaddr) = AllInterfaces::locked_pick_available_https_registrar(lallif).await {
            debug.debug_info(format!("new pledge {} connects to {}", pledgeaddr, target_sockaddr)).await;
            match TcpStream::connect(target_sockaddr).await {
                Ok(mut conn) => {
                    // Bidirectional copy
                    let (n1, n2) = tokio::io::copy_bidirectional(&mut socket, &mut conn).await?;
                    debug.debug_info(format!("copied {} / {} bytes between streams", n1,n2)).await;
                    return Ok(())
                },
                Err(e)   => {
                    debug.debug_error(
                        format!("did not connect to Registrar: {}",e)).await;
                    return Err(e);
                }
            }
        } else {
            debug.debug_info(format!("no available ACP Registrar for new pledge {}", pledgeaddr)).await;
            return Ok(());
        }
    }

    pub async fn start_daemon(ifn: &Interface,
                              lallif: Arc<Mutex<AllInterfaces>>,
                              _invalidate: Arc<Mutex<bool>>) -> Result<Arc<Mutex<JoinInterface>>, rtnetlink::Error> {

        let (ji,js) = JoinInterface::open_ports(ifn.debug.clone(), ifn.ifindex).await.unwrap();

        let jil = Arc::new(Mutex::new(ji));
        let ji2 = jil.clone();
        let debug = ifn.debug.clone();

        // move into variables whichwill get captured into three async threads
        let (_stateless_sock, _stateful_sock, https_listen_sock) = {
            (js.stateless_sock, js.stateful_sock, js.https_sock)
        };

        // jil and debug gets moved into the async loop
        tokio::spawn(async move {
            let mut cnt: u32 = 0;

            loop {
                debug.debug_verbose(format!("{} join loop: {:?}", cnt, https_listen_sock)).await;

                match https_listen_sock.accept().await {
                    Ok((socket, addr)) => {
                        debug.debug_verbose(format!("new pledge client from: {:?} on {:?}",
                                                    addr, socket)).await;
                        let ji3 = jil.clone();
                        let lallif3 = lallif.clone();
                        /* move ji3, lallif3, socket and addr */
                        tokio::spawn(async move {
                            JoinInterface::proxy_https(ji3, lallif3, socket, addr).await.expect("connection failure");
                        });
                    },
                    Err(e) => {
                        debug.debug_error(format!("couldn't get client: {:?}", e)).await;
                        sleep(Duration::from_millis(5000)).await;
                    },
                }
                cnt += 1;
            }

        });

        Ok(ji2)
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

    fn setup_ifn() -> (Interface,AllInterfaces) {
        let (_awriter, all1) = setup_ai();
        let mut ifn = Interface::default(all1.debug.clone());
        ifn.ifindex= 1; // usually lo.
        ifn.ifname = "lo".to_string();
        ifn.ignored= false;
        ifn.mtu    = 1500;
        ifn.oper_state = State::Up;
        ifn.acp_daemon = None;
        ifn.join_daemon = None;
        (ifn, all1)
    }

    fn setup_invalidated_bool() -> Arc<Mutex<bool>> {
        Arc::new(Mutex::new(false))
    }

    async fn async_start_join() -> Result<(), std::io::Error> {
        let (ifn,all1) = setup_ifn();
        let lall = Arc::new(Mutex::new(all1));
        JoinInterface::start_daemon(&ifn, lall.clone(), setup_invalidated_bool()).await.unwrap();
        Ok(())
    }

    #[test]
    fn test_start_join() -> Result<(), std::io::Error> {
        //let (_awriter, mut all1) = setup_ai();
        aw!(async_start_join()).unwrap();
        Ok(())
    }

    async fn async_open_socket() -> Result<(), std::io::Error> {
        let (ifn,_all1) = setup_ifn();
        // ifindex=1, is lo
        let (_aifn,_js) = JoinInterface::open_ports(ifn.debug, 1).await.unwrap();
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
