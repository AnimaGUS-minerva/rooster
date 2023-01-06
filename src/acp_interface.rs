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
// according to the provided list of acp interfaces, joinlink interfaces, and
// interfacs to ignore.
// The lists may include glob(1) wildcards.
//
// Interfaces which match none of the lists are placed into the joinlink interface
// list if the list is empty, otherwise, they are ignored
//

//use std::net::Ipv6Addr;

extern crate moz_cbor as cbor;

use std::net::Ipv6Addr;
use tokio::net::{UdpSocket};
//use std::io::Error;
use std::io::ErrorKind;
use std::net::{SocketAddrV6, IpAddr};
//use std::net::{SocketAddr};
use std::sync::Arc;
use std::time::{SystemTime, Duration};

use futures::lock::Mutex;
use tokio::process::{Command};

use cbor::decoder::decode as cbor_decode;

//use crate::debugoptions::DebugOptions;
//use crate::args::RoosterOptions;
use crate::interface::Interface;
use crate::interface::IfIndex;
use crate::grasp;
use crate::grasp::GraspMessage;
use crate::interfaces::AllInterfaces;
use crate::debugoptions::DebugOptions;
use crate::grasp::GraspMessageType;
use crate::grasp::GraspLocator;

pub const BRSKI_HTTP_OBJECTIVE: &str = "BRSKI";
pub const BRSKI_COAP_OBJECTIVE: &str = "BRSKI_JP";
pub const BRSKI_JPY_OBJECTIVE:  &str = "BRSKI_RJP";

#[derive(Copy, Clone, PartialEq)]
pub enum RegistrarType {
    NoneRegistrar,
    HTTPRegistrar,
    CoAPRegistrar,
    StatelessCoAPRegistrar,
}

pub struct Registrar {
    pub rtype: [RegistrarType; (1+RegistrarType::StatelessCoAPRegistrar as usize)],
    pub addr: IpAddr,
    pub port: u16,
    pub last_announce: SystemTime,
    pub ttl:  Duration
}

impl Registrar {
    pub async fn forward_socket(_incoming: UdpSocket) -> Result<(), std::io::Error> {
        Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "failed".to_string()))
    }
}

pub struct AcpInterface {
    pub sock: UdpSocket,
    pub debug: Arc<DebugOptions>,
    pub registrars: Vec<Registrar>
}

impl AcpInterface {
    pub fn default(sock: UdpSocket, debug: Arc<DebugOptions>) -> AcpInterface {
        AcpInterface {
            sock, debug,
            registrars: vec![]

        }
    }

    pub async fn open_grasp_port(ifn: &Interface,
                                 ifindex: IfIndex) -> Result<AcpInterface, std::io::Error> {
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

                return Ok(AcpInterface::default(recv, ifn.debug.clone()));
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

    pub async fn add_registrar(self: &mut AcpInterface, _cnt: u32, rtype: RegistrarType,
                               v6addr: Ipv6Addr, port_number: u16, ttl: Duration) {

        /* look into list of registrars */
        let mut found = self.registrars.iter_mut().find(|rm| { let r = &**rm;
                                                               r.addr == v6addr &&
                                                               r.port == port_number});
        if let Some(ref mut r) = found {
            r.last_announce = SystemTime::now();
            r.ttl = ttl;
            r.rtype[rtype as usize] = rtype;
        } else {
            let mut newone = Registrar { addr: IpAddr::V6(v6addr), port: port_number,
                                     last_announce: SystemTime::now(),
                                     rtype: [RegistrarType::NoneRegistrar,
                                             RegistrarType::NoneRegistrar,
                                             RegistrarType::NoneRegistrar,
                                             RegistrarType::NoneRegistrar],
                                     ttl: ttl };
            newone.rtype[rtype as usize] = rtype;
            self.registrars.push(newone);
        };
    }

    pub async fn registrar_announce(self: &mut AcpInterface, cnt: u32, graspmessage: GraspMessage) {
        self.debug.debug_verbose(format!("{} grasp mflood[{}] from {}", cnt,
                                         graspmessage.session_id,
                                         graspmessage.initiator)).await;

        let mut objcnt = 1;
        let ttl = Duration::from_millis(graspmessage.ttl.into());
        for objective in graspmessage.objectives {
            let objvaluestr = if let Some(ref value) = objective.objective_value {
                value.clone()
            } else {
                "none".to_string()
            };

            self.debug.debug_verbose(format!("  {}.{} obj: {} ({})", cnt,
                                             objcnt, objective.objective_name,
                                             objvaluestr)).await;
            if let Some(locator) = objective.locator {
                match locator {
                    GraspLocator::O_IPv6_LOCATOR{ v6addr, transport_proto, port_number } => {
                        self.debug.debug_verbose(format!("  {}.{} type:IPv6({}) [{}]:{}", cnt, objcnt,
                                                         transport_proto, v6addr, port_number)).await;
                        match objective.objective_value {
                            Some(ref value) if value == "" || value == "BRSKI" => {
                                self.add_registrar(cnt, RegistrarType::HTTPRegistrar, v6addr,
                                                   port_number, ttl).await
                            },
                            Some(ref value) if value == "BRSKI_JP" => {
                                self.add_registrar(cnt, RegistrarType::CoAPRegistrar, v6addr,
                                                   port_number, ttl).await
                            },
                            Some(ref value) if value == "BRSKI_RJP" => {
                                self.add_registrar(cnt, RegistrarType::StatelessCoAPRegistrar,
                                                   v6addr, port_number, ttl).await
                            },
                            _ => {
                                self.debug.debug_verbose(format!("  {}.{} unknown objective value",
                                                                 cnt, objcnt)).await;
                                return;
                            },
                        }
                    },
                    _ => {
                        self.debug.debug_verbose(format!("  {}.{} other-type {:?}", cnt, objcnt,
                                                         locator)).await;
                        return;
                    }
                };
            }
            objcnt = objcnt + 1;
        }

    }

    pub async fn announce(self: &mut AcpInterface, cnt: u32, graspmessage: GraspMessage) {
        // now we have a graspmessage which we'll do something with!
        self.debug.debug_verbose(format!("{} grasp message: {:?}", cnt, graspmessage)).await;

        if graspmessage.mtype == GraspMessageType::M_FLOOD {
            self.registrar_announce(cnt, graspmessage).await;
        }
    }

    pub async fn start_daemon(ifn: &Interface) -> Result<Arc<Mutex<AcpInterface>>, rtnetlink::Error> {
        let ai = AcpInterface::open_grasp_port(ifn, ifn.ifindex).await.unwrap();

        let ail = Arc::new(Mutex::new(ai));
        let ai2 = ail.clone();

        // ail gets moved into the async loop

        tokio::spawn(async move {
            let mut cnt: u32 = 0;

            loop {
                let mut bufbytes = [0u8; 2048];

                //if debug_graspdaemon {
                //}

                // lock it, read from it and return result
                let (results,debug) = {
                    let ai = ail.lock().await;
                    //println!("listening on GRASP socket {:?}", ai.sock);
                    let res = ai.sock.recv_from(&mut bufbytes).await;
                    let debug = ai.debug.clone();
                    //println!("got answer from GRASP socket {:?}", ai.sock);
                    (res,debug)
                };

                match results {
                    Ok((size, addr)) => {
                        debug.debug_info(format!("{}: grasp daemon read: {} bytes from {}",
                                                 cnt, size, addr)).await;
                        let graspmessage = match cbor_decode(&bufbytes) {
                            Ok(cbor) => {
                                match GraspMessage::decode_grasp_message(cbor) {
                                    Ok(msg) => msg,
                                    err @ _ => {
                                        debug.debug_info(format!("   invalid grasp message: {:?}", err)).await;
                                        continue;
                                    }
                                }
                            },
                            err @ _ => {
                                debug.debug_info(format!("   invalid cbor in message: {:?}", err)).await;
                                continue;
                            }
                        };

                         {
                             let mut ai = ail.lock().await;
                             ai.announce(cnt, graspmessage).await;
                         }
                    }
                    Err(msg) => {
                        debug.debug_info(format!("{} grasp read got error: {:?}", cnt, msg)).await;
                        // deal with socket closed?
                    }
                }

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
    use crate::interface::InterfaceType;
    use crate::grasp::GraspObjective;

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
        ifn.daemon = InterfaceType::Ignored;
        ifn
    }

    async fn async_start_acp() -> Result<(), std::io::Error> {
        let     ifn = setup_ifn();
        AcpInterface::start_daemon(&ifn).await.unwrap();
        Ok(())
    }

    #[test]
    fn test_start_acp() -> Result<(), std::io::Error> {
        //let (_awriter, mut all1) = setup_ai();
        aw!(async_start_acp()).unwrap();
        Ok(())
    }

    async fn async_open_socket() -> Result<(), std::io::Error> {
        let ifn = setup_ifn();
        // ifindex=1, is lo
        let _aifn = AcpInterface::open_grasp_port(&ifn, 1).await.unwrap();
        Ok(())
    }

    #[test]
    fn test_open_socket() -> Result<(), std::io::Error> {
        //let (_awriter, mut all1) = setup_ai();
        aw!(async_open_socket()).unwrap();
        Ok(())
    }

    async fn async_process_mflood1() -> Result<(), std::io::Error> {
        let m1= GraspMessage { mtype: GraspMessageType::M_FLOOD,
                               session_id: 1,
                               initiator: "fda3:79a6:f6ee:0:200:0:6400:1".parse::<Ipv6Addr>().unwrap(),
                               ttl: 180000,
                               objectives: vec![
                                   GraspObjective {
                                       objective_name: "AN_join_registrar".to_string(),
                                       objective_flags: 4, loop_count: 255,
                                       objective_value: Some("".to_string()),
                                       locator: Some(GraspLocator::O_IPv6_LOCATOR {
                                           v6addr: "fda3:79a6:f6ee:0:200:0:6400:1".parse::<Ipv6Addr>().unwrap(),
                                           transport_proto: 6, port_number: 8993 }
                                       )
                                   }
                               ]
        };
        let ifn = setup_ifn();
        let mut aifn = AcpInterface::open_grasp_port(&ifn, 1).await.unwrap();
        aifn.registrar_announce(1, m1).await;
        assert_eq!(aifn.registrars.len(), 1);
        Ok(())
    }

    #[test]
    fn test_process_mflood1() -> Result<(), std::io::Error> {
        aw!(async_process_mflood1()).unwrap();
        Ok(())
    }

    async fn async_process_mflood3() -> Result<(), std::io::Error> {
        let m1= GraspMessage { mtype: GraspMessageType::M_FLOOD,
                               session_id: 1,
                               initiator: "fda3:79a6:f6ee:0:200:0:6400:1".parse::<Ipv6Addr>().unwrap(),
                               ttl: 180000,
                               objectives: vec![
                                   GraspObjective {
                                       objective_name: "AN_join_registrar".to_string(),
                                       objective_flags: 4, loop_count: 255,
                                       objective_value: Some("".to_string()),
                                       locator: Some(GraspLocator::O_IPv6_LOCATOR {
                                           v6addr: "fda3:79a6:f6ee:0:200:0:6400:1".parse::<Ipv6Addr>().unwrap(),
                                           transport_proto: 6, port_number: 8993 }
                                       )
                                   },
                                   GraspObjective {
                                       objective_name: "AN_join_registrar".to_string(),
                                       objective_flags: 4, loop_count: 255,
                                       objective_value: Some(String::from(BRSKI_JPY_OBJECTIVE)),
                                       locator: Some(GraspLocator::O_IPv6_LOCATOR {
                                           v6addr: "fda3:79a6:f6ee:0:200:0:6400:1".parse::<Ipv6Addr>().unwrap(),
                                           transport_proto: 17, port_number: 2345 }
                                       )
                                   },
                                   GraspObjective {
                                       objective_name: "AN_join_registrar".to_string(),
                                       objective_flags: 4, loop_count: 255,
                                       objective_value: Some(String::from(BRSKI_COAP_OBJECTIVE)),
                                       locator: Some(GraspLocator::O_IPv6_LOCATOR {
                                           v6addr: "fda3:79a6:f6ee:0:200:0:6400:1".parse::<Ipv6Addr>().unwrap(),
                                           transport_proto: 17, port_number: 3456 }
                                       )
                                   }
                               ]
        };
        let ifn = setup_ifn();
        let mut aifn = AcpInterface::open_grasp_port(&ifn, 1).await.unwrap();
        aifn.registrar_announce(1, m1).await;
        assert_eq!(aifn.registrars.len(), 3);
        Ok(())
    }

    #[test]
    fn test_process_mflood3() -> Result<(), std::io::Error> {
        aw!(async_process_mflood3()).unwrap();
        Ok(())
    }



}

/*
 * Local Variables:
 * mode: rust
 * compile-command: "cd .. && RUSTFLAGS='-A dead_code -Awarnings' cargo build"
 * End:
 */